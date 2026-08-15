//! Voices handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::{Duration, Instant, RwLock};
use crate::handlers::default_routes_for_voice;
#[cfg(feature = "midi")]
use crate::state::MidiVoicePool;
use crate::state::{State, VoiceRole, VoiceState};
use crate::traits::{VoiceConfig, Voices};
use crate::types::{BusId, ControlBusId, NodeId, ParamMap, VoiceId};
use crate::validation::Validate;
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "midi")]
use crate::midi::{
    pack_note_off, pack_note_on, send_cc_for_param, QueuedMidiEvent, ScheduledMidiEvent,
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

/// Extra margin added on top of a voice's release time before its deferred
/// resources are reclaimed (fallback `/n_free`, route mixers, buses, node
/// IDs). Covers OSC latency plus the envelope's asymptotic tail.
const VOICE_RECLAIM_MARGIN_MS: u64 = 100;

/// Upper bound on the release-derived grace period so a pathological
/// `release` value can't pin buses and node IDs for minutes.
const VOICE_RECLAIM_MAX_RELEASE_S: f32 = 30.0;

/// SFZ voices resolve their release envelope per region at note-on, so the
/// voice-level config can't see it; use a conservative floor so typical
/// `ampeg_release` tails aren't truncated by the fallback free.
const SFZ_RELEASE_GRACE_FLOOR_S: f32 = 1.0;

/// Whether teardown of this voice's nodes can go through the gate-release
/// path (`/n_set gate 0` → release envelope → doneAction=2 self-free).
///
/// SFZ voices spawn `sfz_voice_{mono,stereo}` synths which always carry a
/// gated envelope; everything else is decided by the synthdef's declared
/// params. A synthdef without a `gate` param has no release envelope to
/// trigger, so callers fall back to an immediate `/n_free`.
pub(crate) fn voice_is_gated(config: &VoiceConfig) -> bool {
    if config.sfz_instrument.is_some() {
        return true;
    }
    if config.synthdef.is_empty() {
        return false;
    }
    vibelang_dsp::get_synthdef_param_defaults_arc(&config.synthdef)
        .map(|d| d.contains_key("gate"))
        .unwrap_or(false)
}

/// Grace period to wait after `gate=0` before reclaiming a voice's deferred
/// resources: the voice's effective release time (script param override, else
/// the synthdef's declared default) plus [`VOICE_RECLAIM_MARGIN_MS`].
pub(crate) fn voice_release_grace(config: &VoiceConfig) -> Duration {
    let release = config
        .params
        .get("release")
        .copied()
        .or_else(|| {
            if config.synthdef.is_empty() {
                None
            } else {
                vibelang_dsp::get_synthdef_param_defaults_arc(&config.synthdef)
                    .and_then(|d| d.get("release").copied())
            }
        })
        .unwrap_or(0.0);
    let floor = if config.sfz_instrument.is_some() {
        SFZ_RELEASE_GRACE_FLOOR_S
    } else {
        0.0
    };
    let release = release.max(floor).clamp(0.0, VOICE_RECLAIM_MAX_RELEASE_S);
    Duration::from_secs_f32(release) + Duration::from_millis(VOICE_RECLAIM_MARGIN_MS)
}

/// Deferred teardown unit for gate-released voice nodes.
///
/// Gate-released nodes free themselves via doneAction=2 when their release
/// envelope completes, at a time the runtime can't observe. Their node IDs
/// are therefore NOT recycled at release time (a recycled ID could be handed
/// to a new synth while the old node still exists — commit 8ca4fed's
/// discipline). Instead each release stashes one of these entries; after
/// `grace` the tick loop sends a fallback `/n_free` (a no-op "Node not
/// found" if the node already self-freed, which the backend tolerates) and
/// only then recycles IDs and returns buses to their allocators, so a bus
/// can never be reallocated while a released node is still sounding into it.
struct PendingVoiceReclaim {
    /// When the release was requested (or, for scheduled note steals, the
    /// scheduled sounding time).
    requested_at: Instant,
    /// Per-entry grace derived from the owning voice's release time.
    grace: Duration,
    /// Gate-released synth nodes: fallback-freed and ID-recycled at deadline.
    released_nodes: Vec<NodeId>,
    /// Route mixer / input router nodes kept alive through the release so
    /// the tail keeps flowing to its destination; freed at deadline.
    route_nodes: Vec<NodeId>,
    /// Audio buses (with allocation width) returned to the pool at deadline.
    audio_buses: Vec<(BusId, u8)>,
    /// Control buses returned to the pool at deadline.
    control_buses: Vec<ControlBusId>,
}

/// Handler for voice operations.
pub struct VoicesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Deferred reclaims for gate-released nodes, drained by [`Self::tick`].
    pending_reclaims: Arc<RwLock<Vec<PendingVoiceReclaim>>>,
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
            pending_reclaims: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "midi")]
            midi_outputs: None,
        }
    }

    /// Process pending voice reclaims whose grace period has elapsed.
    ///
    /// Called by the runtime's tick loop (same idiom as
    /// [`crate::handlers::EffectsHandler::tick`]). Sequence per entry:
    /// fallback `/n_free` for released and route nodes, then recycle the
    /// node IDs, then return buses to the allocators — see
    /// [`PendingVoiceReclaim`] for why this order matters.
    pub async fn tick(&self) {
        let now = Instant::now();
        let due: Vec<PendingVoiceReclaim> = {
            let mut pending = self.pending_reclaims.write().await;
            if pending.is_empty() {
                return;
            }
            let mut due = Vec::new();
            let mut remaining = Vec::new();
            for entry in pending.drain(..) {
                // On WASM there is no runtime-driven timing guarantee; free
                // immediately like EffectsHandler does.
                let elapsed = cfg!(target_arch = "wasm32")
                    || now.duration_since(entry.requested_at) >= entry.grace;
                if elapsed {
                    due.push(entry);
                } else {
                    remaining.push(entry);
                }
            }
            *pending = remaining;
            due
        };

        for entry in due {
            for node in entry.released_nodes.iter().chain(entry.route_nodes.iter()) {
                tracing::debug!(
                    "Voice reclaim: grace elapsed, fallback-freeing node {:?}",
                    node
                );
                let _ = self.backend.free_node(*node).await;
            }
            let mut state = self.state.write().await;
            // The /n_free above has been sent, so the IDs are definitively
            // dead and safe to recycle now.
            for node in entry.released_nodes.iter().chain(entry.route_nodes.iter()) {
                state.free_node_id(*node);
            }
            for (bus, channels) in &entry.audio_buses {
                state.free_audio_bus(*bus, *channels);
            }
            for bus in &entry.control_buses {
                state.free_control_bus(*bus);
            }
        }
    }

    /// Queue a deferred reclaim entry.
    async fn push_reclaim(&self, entry: PendingVoiceReclaim) {
        self.pending_reclaims.write().await.push(entry);
    }

    /// Flush still-sounding MIDI pool notes for a detached voice as
    /// NoteOffs to the external device.
    #[cfg(feature = "midi")]
    async fn flush_pool_cleanup(&self, cleanup: Option<(MidiDeviceId, Vec<QueuedMidiEvent>)>) {
        if let Some((device_id, events)) = cleanup {
            for event in events {
                let _ = self.send_midi_event_now(device_id, event).await;
            }
        }
    }

    /// Replace a voice's config with spawn-before-release ordering.
    ///
    /// Reload uses this when a structural change (synthdef, group, or SFZ
    /// instrument) forces a recreate. For gated voices with sounding nodes
    /// the sequence is deterministic:
    ///
    /// 1. detach the old voice's bookkeeping (its nodes keep sounding),
    /// 2. create the new voice — it allocates fresh buses because the old
    ///    ones are still held back for the release tail, so old and new
    ///    can overlap without bleeding into each other,
    /// 3. gate-release the old nodes and queue the deferred reclaim.
    ///
    /// The old tail rings out under the new voice's first trigger instead
    /// of leaving a gap. Gateless voices (and voices with nothing sounding)
    /// keep the legacy delete-then-create sequence, which also preserves
    /// their bus-reuse behaviour across reloads.
    pub async fn recreate(&self, id: VoiceId, config: VoiceConfig) -> Result<()> {
        let old_gated_sounding = {
            let state = self.state.read().await;
            state.voices.get(&id).map(|v| {
                voice_is_gated(&v.config) && !(v.active_nodes.is_empty() && v.note_nodes.is_empty())
            })
        };
        match old_gated_sounding {
            None => return self.create(id, config).await,
            Some(false) => {
                let _ = self.delete(id).await;
                return self.create(id, config).await;
            }
            Some(true) => {}
        }

        let detached = {
            let mut state = self.state.write().await;
            detach_voice(&mut state, id)?
        };

        #[cfg(feature = "midi")]
        self.flush_pool_cleanup(detached.pool_cleanup).await;

        // Spawn-before-release: materialize the new voice first so the old
        // release tail overlaps the new sound instead of leaving a gap.
        let create_result = self.create(id, config).await;

        for node_id in &detached.nodes {
            tracing::debug!(
                "Voice {:?}: recreate - gate-releasing old node {:?}",
                id,
                node_id
            );
            let _ = self.backend.set_param(*node_id, "gate", 0.0).await;
        }
        self.push_reclaim(PendingVoiceReclaim {
            requested_at: Instant::now(),
            grace: detached.grace,
            released_nodes: detached.nodes,
            route_nodes: detached.route_nodes,
            audio_buses: detached.audio_buses,
            control_buses: detached.control_buses,
        })
        .await;

        create_result
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

    /// Send a single Note-On/Note-Off MIDI event to a device immediately.
    ///
    /// Thin wrapper over [`send_midi_event_at`](Self::send_midi_event_at)
    /// with no deadline (send as soon as the output thread picks it up).
    #[cfg(feature = "midi")]
    async fn send_midi_event_now(
        &self,
        device_id: MidiDeviceId,
        event: QueuedMidiEvent,
    ) -> Result<()> {
        self.send_midi_event_at(device_id, event, None).await
    }

    /// Send a single Note-On/Note-Off MIDI event to a device, optionally
    /// scheduled at a future wall-clock time.
    ///
    /// The event is enqueued on the device's output thread, which holds it
    /// in a timestamp-ordered heap and sends it when `at` arrives (~0.1ms
    /// polling precision). This call never sleeps — lookahead dispatch from
    /// the runtime tick computes, enqueues, and returns.
    ///
    /// Prefers the low-latency direct channel; falls back to a sample-accurate
    /// `vibelang_midi_note_{on,off}` synthdef when the direct channel is
    /// unavailable or full. Used by the unified voice-allocation pool path.
    #[cfg(feature = "midi")]
    async fn send_midi_event_at(
        &self,
        device_id: MidiDeviceId,
        event: QueuedMidiEvent,
        at: Option<Instant>,
    ) -> Result<()> {
        // Try direct path first (lower latency).
        match self.get_midi_sender(device_id) {
            Some(sender) => {
                let scheduled = match at {
                    Some(timestamp) => event.clone().at(timestamp),
                    None => event.clone().immediate(),
                };
                if sender.try_send(scheduled).is_ok() {
                    tracing::debug!(
                        "MIDI pool: direct {:?} -> device {:?} (at {:?})",
                        event,
                        device_id,
                        at
                    );
                    return Ok(());
                }
                tracing::warn!(
                    "MIDI pool: direct channel for device {:?} is full — dropping {:?}",
                    device_id,
                    event
                );
            }
            None => {
                // No output sender registered for this id. This is the symptom
                // of a device-id mismatch (the voice's `midi_output` id is not
                // the one the output port was opened under) — the synthdef
                // fallback below only works when a `vibelang_midi_note_{on,off}`
                // synthdef is loaded, which is not the deploy default, so warn
                // loudly rather than silently no-op.
                tracing::warn!(
                    "MIDI pool: no output sender registered for device {:?} — \
                     event {:?} not delivered via the direct path; the device \
                     may not be open or the voice's MIDI output id is stale. \
                     Attempting synthdef fallback (no-op unless a \
                     vibelang_midi_note_* synthdef is loaded).",
                    device_id,
                    event
                );
            }
        }

        // Fallback: sample-accurate MIDI output via synthdef.
        let (synthdef, packed_data) = match &event {
            QueuedMidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => (
                "vibelang_midi_note_on",
                pack_note_on(device_id.0 as u8, *channel, *note, *velocity),
            ),
            QueuedMidiEvent::NoteOff { channel, note } => (
                "vibelang_midi_note_off",
                pack_note_off(device_id.0 as u8, *channel, *note),
            ),
            // The mono note-stack only ever emits NoteOn/NoteOff.
            _ => return Ok(()),
        };

        let mut params = std::collections::HashMap::new();
        params.insert("packed_data".to_string(), packed_data);
        let node_id = { self.state.write().await.alloc_node_id()? };
        if let Err(e) = self
            .backend
            .create_synth_at(
                synthdef,
                node_id,
                NodeId::new(0),
                AddAction::Tail,
                &params,
                &[],
                at,
            )
            .await
        {
            tracing::warn!("MIDI mono: failed to create {} synth: {:?}", synthdef, e);
        }
        Ok(())
    }

    /// Run the unified voice-allocation pool on note-on for a MIDI-output
    /// voice with `n` slots (`poly(1)` is `n == 1`) and emit the resulting
    /// MIDI event sequence (retrigger / free-slot assign / steal with an
    /// optional `NoteOff(stolen)`, see [`midi_pool_note_on`]).
    ///
    /// Pool state advances at dispatch time; the emitted events are enqueued
    /// on the device's output thread with the `at` deadline (up to one
    /// lookahead window ahead) so this call never sleeps. Events emitted by
    /// one pool decision share the deadline and keep their relative order in
    /// the output heap (creation-sequence tie-break).
    #[cfg(feature = "midi")]
    #[allow(clippy::too_many_arguments)]
    async fn midi_pool_note_on(
        &self,
        id: VoiceId,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
        polyphony: usize,
        legato: bool,
        at: Option<Instant>,
    ) -> Result<u64> {
        let (generation, events) = {
            let mut state = self.state.write().await;
            let generation = bump_note_generation(&mut state, id, note);
            let events =
                midi_pool_note_on(&mut state, id, channel, note, velocity, polyphony, legato);
            (generation, events)
        };
        for event in events {
            let _ = self.send_midi_event_at(device_id, event, at).await;
        }
        Ok(generation)
    }

    /// Run the unified voice-allocation pool on note-off: free the released
    /// note's slot, return to the most-recently-stolen still-held note (re-
    /// `NoteOn`), or `NoteOff` when nothing remains in that slot.
    ///
    /// As with [`midi_pool_note_on`](Self::midi_pool_note_on), pool state
    /// advances at dispatch time and the events are enqueued with the `at`
    /// deadline — no inline sleeping.
    #[cfg(feature = "midi")]
    async fn midi_pool_note_off(
        &self,
        id: VoiceId,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        polyphony: usize,
        at: Option<Instant>,
    ) -> Result<()> {
        let events = {
            let mut state = self.state.write().await;
            state.voice_note_generations.remove(&(id, note));
            midi_pool_note_off(&mut state, id, channel, note, polyphony)
        };
        for event in events {
            let _ = self.send_midi_event_at(device_id, event, at).await;
        }
        Ok(())
    }

    /// Resize a MIDI-output voice's allocation pool after a reload changed its
    /// `polyphony`; `NoteOff`s any sounding notes that no longer fit.
    #[cfg(feature = "midi")]
    pub async fn resize_midi_pool(&self, id: VoiceId, polyphony: usize) -> Result<()> {
        let (device_id, events) = {
            let mut state = self.state.write().await;
            let Some(voice) = state.voices.get(&id) else {
                return Ok(());
            };
            let Some(device_id) = voice.config.midi_output else {
                // Not a MIDI-output voice — nothing to do, but drop any stale pool.
                state.midi_voice_pool.remove(&id);
                return Ok(());
            };
            let channel = voice.config.midi_channel;
            let events = midi_pool_resize(&mut state, id, channel, polyphony);
            (device_id, events)
        };
        for event in events {
            let _ = self.send_midi_event_now(device_id, event).await;
        }
        Ok(())
    }

    /// Lookahead-aware note-on.
    ///
    /// MIDI voices enqueue the note on the device's output thread with the
    /// scheduler timestamp as deadline; audio voices forward the timestamp
    /// to the backend for sample-accurate scheduling. Never sleeps.
    /// See [`note_on_at_tracked`](Self::note_on_at_tracked).
    #[cfg(feature = "midi")]
    pub async fn note_on_at(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        timestamp: Option<Instant>,
    ) -> Result<()> {
        self.note_on_at_tracked(id, note, velocity, timestamp)
            .await
            .map(|_| ())
    }

    /// Lookahead-aware note-on that returns the active note generation.
    ///
    /// MIDI-output voices run the pool path at dispatch time and enqueue
    /// the resulting events on the device's output thread with the
    /// scheduler timestamp as deadline (the output thread's heap provides
    /// ~0.1ms-precision delivery). Audio-synth voices instead forward the
    /// timestamp to the backend, which schedules the trigger
    /// sample-accurately (scsynth OSC bundle time-tags). Neither path
    /// sleeps — dispatch from the runtime tick computes, enqueues, and
    /// returns.
    ///
    /// Note: as with the audio path, pool/state bookkeeping (slot
    /// allocation, note generations) happens at dispatch time, up to one
    /// lookahead window (~50ms) before the event sounds.
    #[cfg(feature = "midi")]
    pub async fn note_on_at_tracked(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        timestamp: Option<Instant>,
    ) -> Result<u64> {
        let (midi_output, midi_channel, polyphony, mono_legato) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (
                voice.config.midi_output,
                voice.config.midi_channel,
                voice.config.polyphony as usize,
                voice.config.mono_legato,
            )
        };

        if let Some(device_id) = midi_output {
            let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;
            return self
                .midi_pool_note_on(
                    id,
                    device_id,
                    midi_channel,
                    note,
                    midi_velocity,
                    polyphony,
                    mono_legato,
                    timestamp,
                )
                .await;
        }

        self.note_on_audio_at(id, note, velocity, &ParamMap::new(), timestamp)
            .await?;
        self.current_note_generation(id, note).await
    }

    /// Lookahead-aware note-on with per-note voice parameters.
    ///
    /// Like [`note_on_at`](Self::note_on_at), but also merges `extra_params`
    /// into the synth creation params (and, for MIDI voices, sends them as CC
    /// messages where mappings exist).
    /// Scheduling semantics as in [`note_on_at_tracked`](Self::note_on_at_tracked):
    /// MIDI voices enqueue events (CCs first, then the pool note-on) on the
    /// output thread with the timestamp as deadline; audio voices get a
    /// backend-scheduled (sample-accurate) trigger. Never sleeps.
    #[cfg(feature = "midi")]
    pub async fn note_on_at_with_params(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        timestamp: Option<Instant>,
        extra_params: &ParamMap,
    ) -> Result<()> {
        let (midi_output, midi_channel, polyphony, mono_legato) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (
                voice.config.midi_output,
                voice.config.midi_channel,
                voice.config.polyphony as usize,
                voice.config.mono_legato,
            )
        };

        if let Some(device_id) = midi_output {
            let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;

            // Send extra params as CC where mappings exist, scheduled at the
            // same deadline as the note. They are enqueued before the pool
            // note-on, so the output heap's creation-order tie-break delivers
            // CC-before-NoteOn at the deadline (matching the previous
            // immediate-send ordering).
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
                            let scheduled = match timestamp {
                                Some(at) => cc_event.at(at),
                                None => cc_event.immediate(),
                            };
                            let _ = sender.try_send(scheduled);
                        }
                    }
                }
            }

            // The per-note params are applied as CC above; the note itself
            // goes through the unified voice-allocation pool.
            return self
                .midi_pool_note_on(
                    id,
                    device_id,
                    midi_channel,
                    note,
                    midi_velocity,
                    polyphony,
                    mono_legato,
                    timestamp,
                )
                .await
                .map(|_| ());
        }

        self.note_on_audio_at(id, note, velocity, extra_params, timestamp)
            .await
    }

    fn bend_baseline_updates_for_spawn(
        state: &State,
        id: VoiceId,
        params: &ParamMap,
        bindings: &[(String, BusId)],
    ) -> Vec<(NodeId, f32)> {
        let target = crate::handlers::ParamRouteTarget::Voice(id);
        let mut updates = Vec::new();

        for (param, _) in bindings {
            let Some(value) = params.get(param) else {
                continue;
            };
            let key = (target, param.clone());
            let Some(summer) = state.param_summers.get(&key) else {
                continue;
            };
            let bend_routed = state
                .param_routes_bend
                .values()
                .any(|targets| targets.iter().any(|(t, tp)| *t == target && tp == param));
            if !bend_routed {
                continue;
            }
            let set_routed = state
                .param_routes_set
                .values()
                .any(|targets| targets.iter().any(|(t, tp)| *t == target && tp == param));
            if !set_routed {
                updates.push((summer.node, *value));
            }
        }

        updates
    }

    async fn forward_bend_baselines(&self, updates: &[(NodeId, f32)]) {
        for (summer, value) in updates {
            let _ = self.backend.set_param(*summer, "baseline", *value).await;
        }
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
        // The MIDI-output branch (CC sends + voice-allocation pool) lives in
        // the lookahead variant; `None` means "as soon as possible".
        #[cfg(feature = "midi")]
        return self
            .note_on_at_with_params(id, note, velocity, None, extra_params)
            .await;

        #[cfg(not(feature = "midi"))]
        self.note_on_audio_at(id, note, velocity, extra_params, None)
            .await
    }

    /// Audio-synth note-on, optionally scheduled at a future time.
    ///
    /// This is the shared backend path for all audio-voice note-ons:
    /// immediate (`at = None`) and lookahead-scheduled (`at = Some`, from
    /// the pattern/melody schedulers). A scheduled note-on is sent as one
    /// OSC bundle carrying the `/s_new` plus any modulation `/n_map`s, so
    /// scsynth starts the synth sample-accurately at the tagged time.
    ///
    /// Note: state bookkeeping (note-node tracking, polyphony) happens at
    /// dispatch time, up to one lookahead window (~50ms) before the sound
    /// starts — same as the previous sleep-based dispatch, minus the sleep.
    async fn note_on_audio_at(
        &self,
        id: VoiceId,
        note: u8,
        velocity: f32,
        extra_params: &ParamMap,
        at: Option<Instant>,
    ) -> Result<()> {
        // Gather info and allocate node while holding lock
        let (
            node_id,
            add_target,
            add_action,
            synthdef,
            params,
            old_node,
            steal_gated,
            steal_grace,
            param_bindings,
            bend_baseline_updates,
        ) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            let mut synthdef = voice.config.synthdef.clone();
            let sfz_instrument = voice.config.sfz_instrument;
            let group_node_id = group.node_id;

            // Build routing params first so script/user params can override them.
            let mut params = ParamMap::new();
            params.insert("out".to_string(), group.audio_bus.0 as f32);
            apply_owned_output_bus_params(voice, &mut params);
            apply_voice_input_bus_params(&state, voice, &mut params);

            params.extend(voice.config.params.clone());
            params.insert("freq".to_string(), midi_to_freq(note));
            params.insert("amp".to_string(), velocity);
            params.insert("gate".to_string(), 1.0);

            // Merge per-note extra params (overrides defaults)
            for (k, v) in extra_params {
                params.insert(k.clone(), *v);
            }

            // Convert sample offset/length from seconds to synth params
            if let Some(sample_id) = voice.config.sample_id {
                if let Some(sample_info) = state.samples.get(&sample_id) {
                    params.insert("bufnum".to_string(), sample_info.buffer_id.0 as f32);

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

            // SFZ voices: match the region for this note and apply its
            // bufnum/rate/envelope/loop params; pick mono/stereo synthdef
            // by the sample's channel count. Without this sfz_voice_*
            // spawns with defaults (bufnum=0, rate=1, release=0.01):
            // wrong sample, no repitching, click on note-off.
            if let Some(sfz_id) = sfz_instrument {
                let Some(spawn) =
                    crate::handlers::sfz_note_spawn_params(&mut state, sfz_id, note, velocity)
                else {
                    return Ok(()); // no matching region — skip, don't play buffer 0
                };
                synthdef = spawn.synthdef.to_string();
                for (k, v) in spawn.params {
                    params.insert(k.to_string(), v);
                }
                // Per-note extra params still override region defaults.
                for (k, v) in extra_params {
                    params.insert(k.clone(), *v);
                }
            }

            let node_id = state.alloc_node_id()?;
            bump_note_generation(&mut state, id, note);

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            let old_node = voice.note_nodes.remove(&note);
            voice.note_nodes.insert(note, node_id);
            let steal_gated = voice_is_gated(&voice.config);
            let steal_grace = voice_release_grace(&voice.config);
            // Gateless synths: the replaced same-pitch node is /n_free'd
            // below — recycle its ID. Gate-released nodes keep their ID
            // until the deferred reclaim (doneAction=2 discipline).
            if let Some(old) = old_node {
                if !steal_gated {
                    state.free_node_id(old);
                }
            }
            let param_bindings = state.active_param_bindings_for_voice(id);
            let bend_baseline_updates =
                Self::bend_baseline_updates_for_spawn(&state, id, &params, &param_bindings);
            let (add_target, add_action) = voice_add_target(&state, id, group_node_id);

            (
                node_id,
                add_target,
                add_action,
                synthdef,
                params,
                old_node,
                steal_gated,
                steal_grace,
                param_bindings,
                bend_baseline_updates,
            )
        };

        // Retire the replaced same-pitch node (lock released). Gated synths
        // get gate=0 scheduled at the retrigger time so the old envelope
        // releases exactly when the new note sounds — no click; the node's
        // ID and fallback /n_free are deferred past the release tail (see
        // [`PendingVoiceReclaim`]). Gateless synths keep the immediate
        // /n_free: their recycled node ID may be reallocated before `at`,
        // so the /n_free must have executed by then.
        if let Some(old) = old_node {
            if steal_gated {
                let _ = self.backend.set_param_at(old, "gate", 0.0, at).await;
                self.push_reclaim(PendingVoiceReclaim {
                    requested_at: at.unwrap_or_else(Instant::now),
                    grace: steal_grace,
                    released_nodes: vec![old],
                    route_nodes: Vec::new(),
                    audio_buses: Vec::new(),
                    control_buses: Vec::new(),
                })
                .await;
            } else {
                let _ = self.backend.free_node(old).await;
            }
        }

        // Create synth — scheduled sample-accurately when `at` is set. The
        // modulation /n_map bindings ride in the same scheduling unit since
        // the node doesn't exist until then.
        let param_buses: Vec<(String, u32)> = param_bindings
            .iter()
            .map(|(param, bus)| (param.clone(), bus.raw()))
            .collect();
        self.backend
            .create_synth_at(
                &synthdef,
                node_id,
                add_target,
                add_action,
                &params,
                &param_buses,
                at,
            )
            .await
            .map_err(Error::backend)?;
        self.forward_bend_baselines(&bend_baseline_updates).await;

        Ok(())
    }

    /// Send a note-off scheduled for a specific time.
    ///
    /// Lookahead-aware note-off. MIDI-output voices run the pool path at
    /// dispatch time and enqueue the resulting events on the device's
    /// output thread with the timestamp as deadline; audio-synth voices
    /// forward the timestamp to the backend so the `gate = 0` release
    /// lands sample-accurately (scsynth OSC bundle time-tag). Never sleeps.
    #[cfg(feature = "midi")]
    pub async fn note_off_at(
        &self,
        id: VoiceId,
        note: u8,
        timestamp: Option<Instant>,
    ) -> Result<()> {
        let (midi_output, midi_channel, polyphony) = {
            let state = self.state.read().await;
            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
            (
                voice.config.midi_output,
                voice.config.midi_channel,
                voice.config.polyphony as usize,
            )
        };

        if let Some(device_id) = midi_output {
            return self
                .midi_pool_note_off(id, device_id, midi_channel, note, polyphony, timestamp)
                .await;
        }

        self.note_off_audio_at(id, note, timestamp).await
    }

    /// Audio-synth note-off (`gate = 0`), optionally scheduled at a future
    /// time. Shared by the immediate trait path (`at = None`) and the melody
    /// lookahead scheduler (`at = Some`).
    async fn note_off_audio_at(&self, id: VoiceId, note: u8, at: Option<Instant>) -> Result<()> {
        let node_to_release = {
            let mut state = self.state.write().await;
            state.voice_note_generations.remove(&(id, note));
            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.note_nodes.remove(&note)
        };

        // Release the note (lock released)
        if let Some(node_id) = node_to_release {
            self.backend
                .set_param_at(node_id, "gate", 0.0, at)
                .await
                .map_err(Error::backend)?;
        }

        Ok(())
    }

    /// Release a note only if no newer same-pitch note has superseded it.
    ///
    /// The generation guard is checked at dispatch time (note generations
    /// are also bumped at dispatch time by the note-on paths, so guard and
    /// bump stay ordered by dispatch order); the release itself is then
    /// scheduled at `timestamp` — enqueued on the MIDI output thread or
    /// backend-scheduled (`gate = 0` time-tag) for audio voices. Callers
    /// that need the guard evaluated at the deadline (e.g. the pattern
    /// gate-release task, whose deadline can be far beyond the lookahead
    /// window) delay the call itself and pass `None`.
    #[cfg(feature = "midi")]
    pub async fn note_off_at_if_generation(
        &self,
        id: VoiceId,
        note: u8,
        generation: u64,
        timestamp: Option<Instant>,
    ) -> Result<()> {
        enum Release {
            Audio(Option<NodeId>),
            #[cfg(feature = "midi")]
            Midi(MidiDeviceId, Vec<QueuedMidiEvent>),
        }

        let release = {
            let mut state = self.state.write().await;
            let Some(voice) = state.voices.get(&id) else {
                return Ok(());
            };

            if state.voice_note_generations.get(&(id, note)).copied() != Some(generation) {
                return Ok(());
            }

            let midi_output = voice.config.midi_output;
            let midi_channel = voice.config.midi_channel;
            let polyphony = voice.config.polyphony as usize;

            state.voice_note_generations.remove(&(id, note));

            if let Some(device_id) = midi_output {
                let events = midi_pool_note_off(&mut state, id, midi_channel, note, polyphony);
                Release::Midi(device_id, events)
            } else {
                let node = state
                    .voices
                    .get_mut(&id)
                    .and_then(|voice| voice.note_nodes.remove(&note));
                Release::Audio(node)
            }
        };

        match release {
            Release::Audio(Some(node_id)) => {
                self.backend
                    .set_param_at(node_id, "gate", 0.0, timestamp)
                    .await
                    .map_err(Error::backend)?;
            }
            Release::Audio(None) => {}
            #[cfg(feature = "midi")]
            Release::Midi(device_id, events) => {
                for event in events {
                    let _ = self.send_midi_event_at(device_id, event, timestamp).await;
                }
            }
        }

        Ok(())
    }

    #[cfg(feature = "midi")]
    async fn current_note_generation(&self, id: VoiceId, note: u8) -> Result<u64> {
        let state = self.state.read().await;
        state
            .voice_note_generations
            .get(&(id, note))
            .copied()
            .ok_or(Error::VoiceNotFound(id))
    }
}

fn bump_note_generation(state: &mut State, id: VoiceId, note: u8) -> u64 {
    let generation = state.next_voice_note_generation;
    state.next_voice_note_generation = state.next_voice_note_generation.saturating_add(1).max(1);
    state.voice_note_generations.insert((id, note), generation);
    generation
}

fn clear_voice_note_generations(state: &mut State, id: VoiceId) {
    state
        .voice_note_generations
        .retain(|(voice_id, _), _| *voice_id != id);
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

        // Allocate a bus per declared output port. Ar ports take an audio-bus
        // chunk of `port.channels` consecutive IDs; Kr ports take one control
        // bus from the segregated free list. The rate is implicit in the
        // stored `BusId` — `voice_bus_reclaims` re-resolves it from the
        // synthdef descriptor when releasing.
        let ports = state.synthdef_outputs(&config.synthdef);
        let mut output_buses = Vec::with_capacity(ports.len());
        for port in &ports {
            let bus = match port.rate {
                vibelang_dsp::PortRate::Ar => state.alloc_audio_bus(port.channels)?,
                // Tr ports share the control-bus allocator with Kr — both
                // ride the Out.kr path; only downstream routing differs.
                vibelang_dsp::PortRate::Kr | vibelang_dsp::PortRate::Tr => {
                    BusId::new(state.alloc_control_bus()?.raw())
                }
            };
            output_buses.push((port.name.clone(), bus));
        }

        // Story 5: Install count-based default routes for this voice's ports.
        // Existing entries are NOT overwritten — that path covers the rare case
        // of a recreate that reuses the same voice id without first calling
        // delete (e.g. test setup); explicit user routes flow via ScriptState
        // and override defaults at the merge step in apply_reload.
        let voice_group = config.group;
        let defaults = default_routes_for_voice(voice_group, &ports);
        clear_voice_note_generations(&mut state, id);

        let role = if config.modulator_only {
            VoiceRole::ModulatorOnly
        } else {
            VoiceRole::Audible
        };

        // Store state
        state.voices.insert(
            id,
            VoiceState {
                id,
                config,
                role,
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
                output_buses,
                input_buses: Vec::new(),
            },
        );

        for (port_name, dests) in defaults {
            state.default_routes.entry((id, port_name)).or_insert(dests);
        }

        // Reload can reuse a voice id without an intervening delete; drop any
        // stale voice-allocation pool so it doesn't carry over. If that pool
        // still held sounding notes, flush a NoteOff for each to the device so
        // a recreate-without-delete can't leave a stuck gate. (The normal
        // reload path calls `delete`/`stop`/`graceful_delete` first, all of
        // which already flush — this is the defensive belt for everything else.)
        #[cfg(feature = "midi")]
        let stuck_pool_cleanup = {
            let midi_info = {
                let voice = state.voices.get(&id);
                voice.and_then(|v| v.config.midi_output.map(|d| (d, v.config.midi_channel)))
            };
            match (midi_info, state.midi_voice_pool.contains_key(&id)) {
                (Some((device_id, channel)), true) => {
                    let events = midi_pool_clear(&mut state, id, channel);
                    if !events.is_empty() {
                        tracing::warn!(
                            "Voice {:?}: recreated with {} still-sounding pool note(s) — \
                             flushing NoteOffs to device {:?}",
                            id,
                            events.len(),
                            device_id,
                        );
                    }
                    Some((device_id, events))
                }
                _ => {
                    state.midi_voice_pool.remove(&id);
                    clear_voice_note_generations(&mut state, id);
                    None
                }
            }
        };
        #[cfg(feature = "midi")]
        drop(state);
        #[cfg(feature = "midi")]
        if let Some((device_id, events)) = stuck_pool_cleanup {
            for event in events {
                let _ = self.send_midi_event_now(device_id, event).await;
            }
        }

        Ok(())
    }

    async fn delete(&self, id: VoiceId) -> Result<()> {
        let detached = {
            let mut state = self.state.write().await;
            detach_voice(&mut state, id)?
        };

        // Free all active synth nodes (lock released)
        for node_id in &detached.nodes {
            let _ = self.backend.free_node(*node_id).await;
        }
        // Free per-port route mixer synths so they no longer read from the
        // voice's freed audio bus (avoids stale `In.ar` reads when the bus
        // is recycled to a later voice).
        for node_id in &detached.route_nodes {
            let _ = self.backend.free_node(*node_id).await;
        }

        // The backend teardown is now definitive; only now may the allocator
        // hand these node IDs and buses to a replacement voice.
        {
            let mut state = self.state.write().await;
            reclaim_detached_now(&mut state, &detached);
        }

        // Release any still-sounding pool notes on the external device.
        #[cfg(feature = "midi")]
        self.flush_pool_cleanup(detached.pool_cleanup).await;

        Ok(())
    }

    async fn graceful_delete(&self, id: VoiceId) -> Result<()> {
        let (detached, defer) = {
            let mut state = self.state.write().await;
            let detached = detach_voice(&mut state, id)?;
            // Only gated voices with sounding nodes need the deferred path;
            // everything else reclaims immediately, exactly like `delete`.
            let defer = detached.gated && !detached.nodes.is_empty();
            (detached, defer)
        };

        if !defer {
            for node_id in &detached.nodes {
                let _ = self.backend.free_node(*node_id).await;
            }
            for node_id in &detached.route_nodes {
                let _ = self.backend.free_node(*node_id).await;
            }
            {
                let mut state = self.state.write().await;
                reclaim_detached_now(&mut state, &detached);
            }
            #[cfg(feature = "midi")]
            self.flush_pool_cleanup(detached.pool_cleanup).await;
            return Ok(());
        }

        // Release any still-sounding pool notes on the external device.
        #[cfg(feature = "midi")]
        self.flush_pool_cleanup(detached.pool_cleanup).await;

        // Set gate=0 on all active synth nodes to trigger release envelopes.
        // The synths free themselves via doneAction=2 when the envelope
        // completes — no immediate /n_free, no click. Route mixers and buses
        // stay alive through the release so the tail keeps flowing to its
        // destination; `tick` reclaims them after the grace period.
        for node_id in &detached.nodes {
            tracing::debug!(
                "Voice {:?}: graceful release - setting gate=0 on node {:?}",
                id,
                node_id
            );
            let _ = self.backend.set_param(*node_id, "gate", 0.0).await;
        }
        self.push_reclaim(PendingVoiceReclaim {
            requested_at: Instant::now(),
            grace: detached.grace,
            released_nodes: detached.nodes,
            route_nodes: detached.route_nodes,
            audio_buses: detached.audio_buses,
            control_buses: detached.control_buses,
        })
        .await;

        Ok(())
    }

    async fn trigger(&self, id: VoiceId, params: &ParamMap) -> Result<()> {
        self.trigger_at(id, params, None).await
    }

    async fn trigger_at(&self, id: VoiceId, params: &ParamMap, at: Option<Instant>) -> Result<()> {
        // Gather info and allocate node while holding lock
        let (
            node_id,
            add_target,
            add_action,
            synthdef,
            merged_params,
            old_nodes,
            trig_gated,
            trig_grace,
            choke_free,
            choke_release,
            param_bindings,
            bend_baseline_updates,
        ) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;

            let group = state
                .groups
                .get(&voice.config.group)
                .ok_or(Error::GroupNotFound(voice.config.group))?;

            // Build routing params first so script/user params can override them.
            let mut merged_params = ParamMap::new();
            merged_params.insert("out".to_string(), group.audio_bus.0 as f32);
            apply_owned_output_bus_params(voice, &mut merged_params);
            apply_voice_input_bus_params(&state, voice, &mut merged_params);

            // Merge default params with trigger params
            merged_params.extend(voice.config.params.clone());
            merged_params.extend(params.clone());

            // Convert sample offset/length from seconds to synth params
            if let Some(sample_id) = voice.config.sample_id {
                if let Some(sample_info) = state.samples.get(&sample_id) {
                    merged_params.insert("bufnum".to_string(), sample_info.buffer_id.0 as f32);

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

            let node_id = state.alloc_node_id()?;

            // Handle choke groups: collect nodes to choke from other voices
            // in the same choke group. Gated voices are choked via gate=0 so
            // their release envelope does the fade (e.g. closed hat choking
            // an open hat); gateless ones fall back to an immediate free.
            let mut choke_free: Vec<NodeId> = Vec::new();
            let mut choke_release: Vec<(Duration, Vec<NodeId>)> = Vec::new();
            if let Some(ref choke) = choke_group {
                for (voice_id, other_voice) in state.voices.iter_mut() {
                    // Skip the voice being triggered
                    if *voice_id == id {
                        continue;
                    }
                    // Check if in same choke group
                    if other_voice.config.choke_group.as_ref() == Some(choke) {
                        // Collect all active nodes to choke
                        let mut nodes = std::mem::take(&mut other_voice.active_nodes);
                        nodes.extend(other_voice.note_nodes.drain().map(|(_, n)| n));
                        if nodes.is_empty() {
                            continue;
                        }
                        if voice_is_gated(&other_voice.config) {
                            choke_release.push((voice_release_grace(&other_voice.config), nodes));
                        } else {
                            choke_free.extend(nodes);
                        }
                    }
                }
            }

            // Update voice state
            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            voice.active_nodes.push(node_id);
            let trig_gated = voice_is_gated(&voice.config);
            let trig_grace = voice_release_grace(&voice.config);

            // Handle round-robin: add `rr` parameter and increment position
            if round_robin_count > 0 {
                merged_params.insert("rr".to_string(), voice.round_robin_position as f32);
                voice.round_robin_position = (voice.round_robin_position + 1) % round_robin_count;
            }

            // Collect nodes displaced by the polyphony limit
            let mut old_nodes = Vec::new();
            while voice.active_nodes.len() > polyphony {
                if let Some(old_node) = voice.active_nodes.first().copied() {
                    voice.active_nodes.remove(0);
                    old_nodes.push(old_node);
                }
            }
            // Recycle node IDs only for nodes taking the immediate-free
            // path — those are explicitly /n_free'd below, so the IDs are
            // definitively done. Gate-released nodes keep their IDs until
            // the deferred reclaim (doneAction=2 discipline).
            if !trig_gated {
                for node in old_nodes.iter() {
                    state.free_node_id(*node);
                }
            }
            for node in choke_free.iter() {
                state.free_node_id(*node);
            }
            let param_bindings = state.active_param_bindings_for_voice(id);
            let bend_baseline_updates =
                Self::bend_baseline_updates_for_spawn(&state, id, &merged_params, &param_bindings);
            let (add_target, add_action) = voice_add_target(&state, id, group_node_id);

            (
                node_id,
                add_target,
                add_action,
                synthdef,
                merged_params,
                old_nodes,
                trig_gated,
                trig_grace,
                choke_free,
                choke_release,
                param_bindings,
                bend_baseline_updates,
            )
        };

        // Choke nodes from other voices in the same choke group (lock
        // released). Gateless frees stay immediate even for scheduled
        // triggers: their node IDs were recycled above and may be
        // reallocated before `at`, so the /n_free must have executed by
        // then. Gated chokes are scheduled at the trigger time so the choke
        // lands exactly when the new note sounds.
        for choke_node in choke_free {
            let _ = self.backend.free_node(choke_node).await;
        }
        for (grace, nodes) in choke_release {
            for node in &nodes {
                let _ = self.backend.set_param_at(*node, "gate", 0.0, at).await;
            }
            self.push_reclaim(PendingVoiceReclaim {
                requested_at: at.unwrap_or_else(Instant::now),
                grace,
                released_nodes: nodes,
                route_nodes: Vec::new(),
                audio_buses: Vec::new(),
                control_buses: Vec::new(),
            })
            .await;
        }

        // Create synth in backend — scheduled sample-accurately when `at`
        // is set. Modulation /n_map bindings ride in the same scheduling
        // unit since the node doesn't exist until then.
        let param_buses: Vec<(String, u32)> = param_bindings
            .iter()
            .map(|(param, bus)| (param.clone(), bus.raw()))
            .collect();
        self.backend
            .create_synth_at(
                &synthdef,
                node_id,
                add_target,
                add_action,
                &merged_params,
                &param_buses,
                at,
            )
            .await
            .map_err(Error::backend)?;
        self.forward_bend_baselines(&bend_baseline_updates).await;

        // Retire nodes displaced by the polyphony limit — gate-released for
        // gated synths (the stolen note tails out under the new one),
        // immediate /n_free otherwise (see choke note above).
        if trig_gated {
            if !old_nodes.is_empty() {
                for node in &old_nodes {
                    let _ = self.backend.set_param_at(*node, "gate", 0.0, at).await;
                }
                self.push_reclaim(PendingVoiceReclaim {
                    requested_at: at.unwrap_or_else(Instant::now),
                    grace: trig_grace,
                    released_nodes: old_nodes,
                    route_nodes: Vec::new(),
                    audio_buses: Vec::new(),
                    control_buses: Vec::new(),
                })
                .await;
            }
        } else {
            for old_node in old_nodes {
                let _ = self.backend.free_node(old_node).await;
            }
        }

        Ok(())
    }

    async fn stop(&self, id: VoiceId) -> Result<()> {
        #[cfg(feature = "midi")]
        let (nodes, gated, grace, pool_cleanup) = {
            let mut state = self.state.write().await;

            let (midi_output, midi_channel) = {
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                (voice.config.midi_output, voice.config.midi_channel)
            };
            clear_voice_note_generations(&mut state, id);
            let pool_events = midi_pool_clear(&mut state, id, midi_channel);
            let pool_cleanup = midi_output.map(|dev| (dev, pool_events));

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            let gated = voice_is_gated(&voice.config);
            let grace = voice_release_grace(&voice.config);
            let mut nodes: Vec<NodeId> = voice.active_nodes.drain(..).collect();
            nodes.extend(voice.note_nodes.drain().map(|(_, node)| node));
            // Gateless path: every node is explicitly /n_free'd below, so
            // recycle the IDs now. Gate-released nodes keep theirs until
            // the deferred reclaim.
            if !gated {
                for node in &nodes {
                    state.free_node_id(*node);
                }
            }
            (nodes, gated, grace, pool_cleanup)
        };
        #[cfg(not(feature = "midi"))]
        let (nodes, gated, grace) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;
            let gated = voice_is_gated(&voice.config);
            let grace = voice_release_grace(&voice.config);
            let mut nodes: Vec<NodeId> = voice.active_nodes.drain(..).collect();
            nodes.extend(voice.note_nodes.drain().map(|(_, node)| node));
            if !gated {
                for node in &nodes {
                    state.free_node_id(*node);
                }
            }
            (nodes, gated, grace)
        };

        // Release any still-sounding pool notes on the external device.
        #[cfg(feature = "midi")]
        self.flush_pool_cleanup(pool_cleanup).await;

        if nodes.is_empty() {
            return Ok(());
        }

        // Stop via gate-release where the synthdef supports it: the release
        // envelope tails out and the node frees itself (doneAction=2); the
        // fallback /n_free and ID recycle happen after the grace period.
        if gated {
            for node_id in &nodes {
                let _ = self.backend.set_param(*node_id, "gate", 0.0).await;
            }
            self.push_reclaim(PendingVoiceReclaim {
                requested_at: Instant::now(),
                grace,
                released_nodes: nodes,
                route_nodes: Vec::new(),
                audio_buses: Vec::new(),
                control_buses: Vec::new(),
            })
            .await;
        } else {
            for node_id in nodes {
                let _ = self.backend.free_node(node_id).await;
            }
        }

        Ok(())
    }

    async fn note_on(&self, id: VoiceId, note: u8, velocity: f32) -> Result<()> {
        // Check for MIDI output first — route through the voice-allocation pool.
        #[cfg(feature = "midi")]
        {
            let (midi_output, midi_channel, polyphony, mono_legato) = {
                let state = self.state.read().await;
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                (
                    voice.config.midi_output,
                    voice.config.midi_channel,
                    voice.config.polyphony as usize,
                    voice.config.mono_legato,
                )
            };

            if let Some(device_id) = midi_output {
                let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0) as u8;
                return self
                    .midi_pool_note_on(
                        id,
                        device_id,
                        midi_channel,
                        note,
                        midi_velocity,
                        polyphony,
                        mono_legato,
                        None,
                    )
                    .await
                    .map(|_| ());
            }
        }

        self.note_on_audio_at(id, note, velocity, &ParamMap::new(), None)
            .await
    }

    async fn note_off(&self, id: VoiceId, note: u8) -> Result<()> {
        // Check for MIDI output first — route through the voice-allocation pool.
        #[cfg(feature = "midi")]
        {
            let (midi_output, midi_channel, polyphony) = {
                let state = self.state.read().await;
                let voice = state.voices.get(&id).ok_or(Error::VoiceNotFound(id))?;
                (
                    voice.config.midi_output,
                    voice.config.midi_channel,
                    voice.config.polyphony as usize,
                )
            };

            if let Some(device_id) = midi_output {
                return self
                    .midi_pool_note_off(id, device_id, midi_channel, note, polyphony, None)
                    .await;
            }
        }

        self.note_off_audio_at(id, note, None).await
    }

    async fn mute(&self, id: VoiceId, muted: bool) -> Result<()> {
        let mut state = self.state.write().await;

        let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

        voice.config.muted = muted;

        Ok(())
    }

    async fn set_param(&self, id: VoiceId, param: &str, value: f32) -> Result<()> {
        // Get all active nodes, voice config, and update the default param value.
        // Multi-output v2 split: if the param is BEND-routed, also forward the
        // value to the summer's `baseline` so the user's set_param behaves as
        // a static center under whatever modulators are riding on top. If
        // it's SET-routed, the /n_map mapping is silently masking this set
        // until the user removes the route — log a debug note in case the
        // user hits a "why doesn't my knob do anything?" moment.
        #[cfg(feature = "midi")]
        let (nodes, voice_config, summer_node, set_routed): (
            Vec<NodeId>,
            VoiceConfig,
            Option<NodeId>,
            bool,
        ) = {
            let mut state = self.state.write().await;

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            voice.config.params.insert(param.to_string(), value);

            let mut nodes: Vec<NodeId> = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());
            let voice_config = voice.config.clone();

            let target = crate::handlers::ParamRouteTarget::Voice(id);
            let key = (target, param.to_string());
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

            let voice = state.voices.get_mut(&id).ok_or(Error::VoiceNotFound(id))?;

            voice.config.params.insert(param.to_string(), value);

            let mut nodes: Vec<NodeId> = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());

            let target = crate::handlers::ParamRouteTarget::Voice(id);
            let key = (target, param.to_string());
            let summer_node = state.param_summers.get(&key).map(|s| s.node);
            let set_routed = state
                .param_routes_set
                .values()
                .any(|targets| targets.iter().any(|(t, tp)| *t == target && tp == param));

            (nodes, summer_node, set_routed)
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

        if set_routed {
            tracing::debug!(
                "set_param on voice {:?} param '{}': param is routed via .to_param \
                 (SET) — the source's /n_map is overriding this value until the \
                 route is removed",
                id,
                param,
            );
        }

        // Set param on all active synths (lock released).
        // For MIDI voices, this also updates the scsynth nodes (if any).
        for node_id in nodes {
            let _ = self.backend.set_param(node_id, param, value).await;
        }

        // BEND-path forwarding: if the target is `.modulate_by`-routed, push
        // the new value into the summer's `baseline` control so the user's
        // set_param shifts the static center. The summer's intermediate bus
        // already feeds the target via /n_map, so this is sufficient.
        //
        // SET-routed targets share the same summer infrastructure but pin
        // `baseline=0` (the source signal "replaces" the user's value),
        // so we skip the forwarding when `set_routed` — pushing baseline
        // there would break the SET semantic.
        if let Some(summer) = summer_node {
            if !set_routed {
                let _ = self.backend.set_param(summer, "baseline", value).await;
            }
        }

        Ok(())
    }
}

/// Convert MIDI note number to frequency in Hz.
fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

/// Look up (creating if needed) the [`MidiVoicePool`] for a MIDI-output voice
/// with `n` slots, repairing its size if `polyphony` changed without a
/// dedicated resize (defensive — reload normally calls [`midi_pool_resize`]).
#[cfg(feature = "midi")]
fn midi_pool_entry(state: &mut State, id: VoiceId, n: usize) -> &mut MidiVoicePool {
    let n = n.max(1);
    let pool = state
        .midi_voice_pool
        .entry(id)
        .or_insert_with(|| MidiVoicePool {
            slots: vec![None; n],
            overflow: Vec::new(),
            alloc_order: Vec::new(),
        });
    if pool.slots.len() != n {
        // Out-of-band size mismatch — grow with free slots; truncating is
        // left to `midi_pool_resize` which can emit the needed NoteOffs.
        if pool.slots.len() < n {
            pool.slots.resize(n, None);
        }
    }
    pool
}

/// Unified voice-allocation pool: note-on side.
///
/// Drives the `n`-slot pool for a MIDI-output voice (`poly(1)` is `n == 1`)
/// and returns the MIDI events to emit in order. Re-pressing a sounding note
/// retriggers it (`NoteOn` re-sent). A free slot takes the new note directly.
/// When every slot is full the oldest-allocated slot is stolen: its note is
/// pushed to the overflow stack and the events are `NoteOff(stolen)` (skipped
/// when `legato`, so portamento synths slur) then `NoteOn(new)`.
#[cfg(feature = "midi")]
fn midi_pool_note_on(
    state: &mut State,
    id: VoiceId,
    channel: u8,
    note: u8,
    velocity: u8,
    n: usize,
    legato: bool,
) -> Vec<QueuedMidiEvent> {
    let pool = midi_pool_entry(state, id, n);

    // 1. Already sounding in a slot → retrigger, refresh allocation order.
    if let Some(slot) = pool
        .slots
        .iter()
        .position(|s| matches!(s, Some((sn, _)) if *sn == note))
    {
        pool.slots[slot] = Some((note, velocity));
        pool.alloc_order.retain(|&i| i != slot);
        pool.alloc_order.push(slot);
        return vec![QueuedMidiEvent::NoteOn {
            channel,
            note,
            velocity,
        }];
    }

    // Re-pressing a held-but-stolen note: drop the stale overflow entry so it
    // is not tracked twice once it lands in a slot.
    pool.overflow.retain(|&(n_, _)| n_ != note);

    // 2. Free slot → assign directly.
    if let Some(slot) = pool.slots.iter().position(|s| s.is_none()) {
        pool.slots[slot] = Some((note, velocity));
        pool.alloc_order.push(slot);
        return vec![QueuedMidiEvent::NoteOn {
            channel,
            note,
            velocity,
        }];
    }

    // 3. All slots full → steal the oldest-allocated slot.
    let Some(&slot) = pool.alloc_order.first() else {
        // Degenerate zero-slot pool (shouldn't happen — polyphony >= 1).
        return vec![QueuedMidiEvent::NoteOn {
            channel,
            note,
            velocity,
        }];
    };
    let mut events = Vec::new();
    if let Some((stolen_note, stolen_vel)) = pool.slots[slot] {
        pool.overflow.push((stolen_note, stolen_vel));
        if !legato {
            events.push(QueuedMidiEvent::NoteOff {
                channel,
                note: stolen_note,
            });
        }
    }
    pool.slots[slot] = Some((note, velocity));
    pool.alloc_order.remove(0);
    pool.alloc_order.push(slot);
    events.push(QueuedMidiEvent::NoteOn {
        channel,
        note,
        velocity,
    });
    events
}

/// Unified voice-allocation pool: note-off side.
///
/// Releasing a sounding note frees its slot; if any note is still held in the
/// overflow stack the most-recently-stolen one is revived into that slot — an
/// explicit `NoteOff` for the released note is emitted *before* the revive's
/// `NoteOn` (mono and poly alike: a mono synth with a note-priority stack keeps
/// the released note "held" and would fall back to it — gate stuck — if it
/// never sees the `NoteOff`). With nothing in the overflow stack a plain
/// `NoteOff` is emitted. Releasing a note that was tracked in the
/// overflow stack just drops it from there. Releasing a note we don't track at
/// all still emits a `NoteOff` to the device (defensive — a redundant NoteOff
/// is harmless, and it stops a permanently stuck gate if the pool bookkeeping
/// ever desyncs, e.g. the pool was cleared by a reload between the NoteOn and
/// this NoteOff).
#[cfg(feature = "midi")]
fn midi_pool_note_off(
    state: &mut State,
    id: VoiceId,
    channel: u8,
    note: u8,
    n: usize,
) -> Vec<QueuedMidiEvent> {
    let pool = midi_pool_entry(state, id, n);
    let mut events = Vec::new();

    if let Some(slot) = pool
        .slots
        .iter()
        .position(|s| matches!(s, Some((sn, _)) if *sn == note))
    {
        pool.slots[slot] = None;
        pool.alloc_order.retain(|&i| i != slot);
        if let Some((rev_note, rev_vel)) = pool.overflow.pop() {
            // Return-to-held: bring the most-recently-stolen still-held note
            // back into this slot. Emit an explicit NoteOff for the released
            // note *before* the revived note's NoteOn — even on a mono
            // destination. A mono synth with a note-priority stack (e.g. the
            // Behringer Model 15) keeps the released note in its held-note
            // stack if it never sees a NoteOff for it; when the revived note is
            // later released the synth falls back to that ghost note and its
            // gate stays open forever. The pre-NoteOff retriggers the envelope
            // on the fallback, which is the expected non-legato behaviour.
            events.push(QueuedMidiEvent::NoteOff { channel, note });
            pool.slots[slot] = Some((rev_note, rev_vel));
            pool.alloc_order.push(slot);
            tracing::debug!(
                "MIDI pool: note_off voice={:?} note={} → slot {} freed, revived held note {} (vel {}); \
                 explicit NoteOff for released note; overflow now {} held, slots={:?}",
                id, note, slot, rev_note, rev_vel, pool.overflow.len(), pool.slots,
            );
            events.push(QueuedMidiEvent::NoteOn {
                channel,
                note: rev_note,
                velocity: rev_vel,
            });
        } else {
            tracing::debug!(
                "MIDI pool: note_off voice={:?} note={} → slot {} freed, NoteOff emitted; \
                 overflow {} held, slots={:?}",
                id,
                note,
                slot,
                pool.overflow.len(),
                pool.slots,
            );
            events.push(QueuedMidiEvent::NoteOff { channel, note });
        }
    } else if pool.overflow.iter().any(|&(n_, _)| n_ == note) {
        // Held-but-stolen note released → drop it from the overflow stack. Its
        // original NoteOn was already matched by a NoteOff at steal time.
        pool.overflow.retain(|&(n_, _)| n_ != note);
        tracing::debug!(
            "MIDI pool: note_off voice={:?} note={} → was held-but-stolen, dropped from overflow; \
             overflow now {} held, slots={:?}",
            id,
            note,
            pool.overflow.len(),
            pool.slots,
        );
    } else {
        // Untracked note released. We have no record of it sounding, but the
        // device might — emit a defensive NoteOff so a desync can't leave the
        // gate stuck open.
        tracing::debug!(
            "MIDI pool: note_off voice={:?} note={} → UNTRACKED (pool desync?); \
             emitting defensive NoteOff; overflow {} held, slots={:?}",
            id,
            note,
            pool.overflow.len(),
            pool.slots,
        );
        events.push(QueuedMidiEvent::NoteOff { channel, note });
    }

    let empty = pool.overflow.is_empty() && pool.slots.iter().all(|s| s.is_none());
    if empty {
        state.midi_voice_pool.remove(&id);
    }
    events
}

/// Tear down the voice-allocation pool for `id` (voice stop / delete / reload):
/// returns a `NoteOff` for every still-sounding slot and clears all state.
#[cfg(feature = "midi")]
fn midi_pool_clear(state: &mut State, id: VoiceId, channel: u8) -> Vec<QueuedMidiEvent> {
    let mut events = Vec::new();
    if let Some(pool) = state.midi_voice_pool.remove(&id) {
        for idx in pool.alloc_order {
            if let Some((note, _)) = pool.slots[idx] {
                events.push(QueuedMidiEvent::NoteOff { channel, note });
            }
        }
    }
    events
}

/// Resize the voice-allocation pool for `id` to `n` slots (reload changed
/// `polyphony`). Growing just adds free slots — no events. Shrinking keeps the
/// `n` most-recently-allocated sounding notes and `NoteOff`s the rest; the
/// overflow stack of held-but-stolen notes is preserved untouched.
#[cfg(feature = "midi")]
fn midi_pool_resize(state: &mut State, id: VoiceId, channel: u8, n: usize) -> Vec<QueuedMidiEvent> {
    let n = n.max(1);
    let Some(pool) = state.midi_voice_pool.get_mut(&id) else {
        return Vec::new();
    };
    if pool.slots.len() == n {
        return Vec::new();
    }

    // Sounding notes, oldest-allocated first.
    let sounding: Vec<(u8, u8)> = pool
        .alloc_order
        .iter()
        .filter_map(|&i| pool.slots.get(i).copied().flatten())
        .collect();

    let mut events = Vec::new();
    let keep_from = sounding.len().saturating_sub(n);
    for &(note, _) in &sounding[..keep_from] {
        events.push(QueuedMidiEvent::NoteOff { channel, note });
    }
    let kept = &sounding[keep_from..];

    let mut slots = vec![None; n];
    let mut alloc_order = Vec::with_capacity(kept.len());
    for (i, &nv) in kept.iter().enumerate() {
        slots[i] = Some(nv);
        alloc_order.push(i);
    }
    pool.slots = slots;
    pool.alloc_order = alloc_order;
    events
}

/// Resolve every bus owned by `voice` into reclaim lists, without freeing
/// anything yet.
///
/// The synthdef descriptor is resolved HERE, at detach time, not at the
/// deferred reclaim deadline — reload tears voices down before changing
/// synthdef metadata, so the descriptor is stable now but may have been
/// redeclared by the time the grace period elapses.
fn voice_bus_reclaims(state: &State, voice: &VoiceState) -> (Vec<(BusId, u8)>, Vec<ControlBusId>) {
    let mut audio = Vec::new();
    let mut control = Vec::new();
    if !voice.output_buses.is_empty() {
        let ports = state.synthdef_outputs(&voice.config.synthdef);
        for (port_name, bus_id) in &voice.output_buses {
            let port = ports.iter().find(|p| p.name == *port_name);
            match port.map(|p| p.rate).unwrap_or(vibelang_dsp::PortRate::Ar) {
                vibelang_dsp::PortRate::Ar => {
                    // Unknown ports (synthdef redeclared mid-flight) fall
                    // back to the legacy stereo Ar shape.
                    audio.push((*bus_id, port.map(|p| p.channels).unwrap_or(2)));
                }
                // Tr ports return their bus to the control-bus free list,
                // same as Kr — both were allocated from there at create time.
                vibelang_dsp::PortRate::Kr | vibelang_dsp::PortRate::Tr => {
                    control.push(ControlBusId::new(bus_id.raw()));
                }
            }
        }
    }
    if !voice.input_buses.is_empty() {
        let ports = state.synthdef_inputs(&voice.config.synthdef);
        for (port_name, bus_id) in &voice.input_buses {
            let channels = ports
                .iter()
                .find(|p| p.name == *port_name)
                .map(|p| p.channels)
                .unwrap_or(2);
            audio.push((*bus_id, channels));
        }
    }
    (audio, control)
}

/// Everything a removed voice owned, captured under the state lock with no
/// backend traffic yet. Produced by [`detach_voice`]; the caller decides
/// whether teardown is immediate (gateless) or gate-released + deferred.
struct DetachedVoice {
    /// Sounding synth nodes (`active_nodes` + `note_nodes`).
    nodes: Vec<NodeId>,
    /// Route mixer and input router nodes owned by the voice.
    route_nodes: Vec<NodeId>,
    audio_buses: Vec<(BusId, u8)>,
    control_buses: Vec<ControlBusId>,
    /// Whether the voice's synths honour `gate=0` release.
    gated: bool,
    /// Release-derived grace for deferred reclaim.
    grace: Duration,
    /// Still-sounding MIDI pool notes to flush as NoteOffs.
    #[cfg(feature = "midi")]
    pool_cleanup: Option<(MidiDeviceId, Vec<QueuedMidiEvent>)>,
}

/// Remove `id` from the state and capture everything it owned.
///
/// Pure bookkeeping: no backend traffic, no node-ID recycling, no bus frees —
/// the caller sequences those, either immediately (hard delete, gateless
/// voices) or after the release grace (gate-released voices).
fn detach_voice(state: &mut State, id: VoiceId) -> Result<DetachedVoice> {
    let voice = state.voices.remove(&id).ok_or(Error::VoiceNotFound(id))?;
    let (audio_buses, control_buses) = voice_bus_reclaims(state, &voice);
    let mut route_nodes = state.take_voice_route_nodes(id);
    route_nodes.extend(state.take_voice_input_route_nodes(id));
    state.take_voice_param_routes(id);
    // Story 5: drop the voice's default routes so the next reload's merge
    // step doesn't carry them forward against a deleted voice.
    state.take_voice_default_routes(id);
    clear_voice_note_generations(state, id);
    #[cfg(feature = "midi")]
    let pool_cleanup = {
        let midi_channel = voice.config.midi_channel;
        let pool_events = midi_pool_clear(state, id, midi_channel);
        voice.config.midi_output.map(|dev| (dev, pool_events))
    };
    let gated = voice_is_gated(&voice.config);
    let grace = voice_release_grace(&voice.config);
    let mut nodes = voice.active_nodes;
    nodes.extend(voice.note_nodes.into_values());
    Ok(DetachedVoice {
        nodes,
        route_nodes,
        audio_buses,
        control_buses,
        gated,
        grace,
        #[cfg(feature = "midi")]
        pool_cleanup,
    })
}

/// Immediately recycle a detached voice's node IDs and buses.
///
/// Only valid when the caller is about to `/n_free` every node in the same
/// operation (hard delete / gateless teardown) — gate-released voices must
/// go through the deferred [`PendingVoiceReclaim`] path instead.
fn reclaim_detached_now(state: &mut State, detached: &DetachedVoice) {
    for node in detached.nodes.iter().chain(detached.route_nodes.iter()) {
        state.free_node_id(*node);
    }
    for (bus, channels) in &detached.audio_buses {
        state.free_audio_bus(*bus, *channels);
    }
    for bus in &detached.control_buses {
        state.free_control_bus(*bus);
    }
}

fn apply_owned_output_bus_params(voice: &VoiceState, params: &mut ParamMap) {
    let synthdef_params = vibelang_dsp::get_synthdef_param_defaults_arc(&voice.config.synthdef);
    let has = |k: &str| {
        synthdef_params
            .as_ref()
            .map(|d| d.contains_key(k))
            .unwrap_or(false)
    };
    if has("out0") {
        for (index, (_, bus)) in voice.output_buses.iter().enumerate() {
            params.insert(format!("out{}", index), bus.raw() as f32);
        }
    } else if has("out") {
        if let Some((_, bus)) = voice.output_buses.first() {
            params.insert("out".to_string(), bus.raw() as f32);
        }
    }
}

fn voice_add_target(
    state: &State,
    voice_id: VoiceId,
    group_node_id: NodeId,
) -> (NodeId, AddAction) {
    state
        .input_route_synths
        .iter()
        .filter_map(|((target_voice_id, _, _), node_id)| {
            (*target_voice_id == voice_id).then_some(*node_id)
        })
        .min_by_key(|node_id| node_id.raw())
        .map(|node_id| (node_id, AddAction::After))
        .unwrap_or((group_node_id, AddAction::Head))
}

fn apply_voice_input_bus_params(state: &State, voice: &VoiceState, params: &mut ParamMap) {
    if voice.input_buses.is_empty() {
        return;
    }
    for (index, input) in state
        .synthdef_inputs(&voice.config.synthdef)
        .iter()
        .enumerate()
    {
        if let Some((_, bus)) = voice
            .input_buses
            .iter()
            .find(|(name, _)| name == &input.name)
        {
            params.insert(
                vibelang_dsp::builder::input_bus_param_name(index),
                bus.raw() as f32,
            );
        }
    }
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
    use std::sync::Mutex;

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
    #[derive(Clone, Debug)]
    struct CreateSynthCall {
        #[allow(dead_code)]
        node: NodeId,
        target: NodeId,
        action: AddAction,
        #[allow(dead_code)]
        params: ParamMap,
    }

    struct MockBackend {
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
        synth_create_params: Mutex<Vec<ParamMap>>,
        synth_create_calls: Mutex<Vec<CreateSynthCall>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
                synth_create_params: Mutex::new(Vec::new()),
                synth_create_calls: Mutex::new(Vec::new()),
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

        fn synth_create_params(&self) -> Vec<ParamMap> {
            self.synth_create_params.lock().unwrap().clone()
        }

        fn synth_create_calls(&self) -> Vec<CreateSynthCall> {
            self.synth_create_calls.lock().unwrap().clone()
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
            node: NodeId,
            target: NodeId,
            action: AddAction,
            params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.synths_created.fetch_add(1, Ordering::Relaxed);
            self.synth_create_params
                .lock()
                .unwrap()
                .push(params.clone());
            self.synth_create_calls
                .lock()
                .unwrap()
                .push(CreateSynthCall {
                    node,
                    target,
                    action,
                    params: params.clone(),
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
                output_channels: None,
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
    // Story 5: Default-routes installation at voice create
    // =========================================================================

    /// Register a synthdef plus its declared OutputPort set so voice creation
    /// resolves the per-port count correctly (instead of falling back to the
    /// implicit legacy stereo `out`).
    async fn register_synthdef_with_ports(
        state: &Arc<RwLock<State>>,
        name: &str,
        ports: Vec<vibelang_dsp::OutputPort>,
    ) {
        let mut s = state.write().await;
        s.synthdefs.insert(name.to_string());
        s.synthdef_outputs.insert(name.to_string(), ports);
    }

    #[tokio::test]
    async fn create_voice_one_mono_port_installs_pan_default_into_group() {
        // 1 mono port → default-routed into the voice's group. The
        // `port_to_group_link_1` mixer (mono dup) realises the pan-mono.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "mono_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let group = GroupId::new(1);
        handler
            .create(voice_id, VoiceConfig::new("v", "mono_synth", group))
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(s.default_routes.len(), 1);
        assert_eq!(
            s.default_routes[&(voice_id, "out".to_string())],
            vec![crate::handlers::RouteDest::Group(group)],
        );
    }

    #[tokio::test]
    async fn create_voice_one_stereo_port_installs_straight_default_into_group() {
        // 1 stereo port → straight L/R into group (today's behaviour).
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "stereo_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let voice_id = VoiceId::new(2);
        let group = GroupId::new(1);
        handler
            .create(voice_id, VoiceConfig::new("v", "stereo_synth", group))
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(s.default_routes.len(), 1);
        assert_eq!(
            s.default_routes[&(voice_id, "out".to_string())],
            vec![crate::handlers::RouteDest::Group(group)],
        );
    }

    #[tokio::test]
    async fn create_voice_two_mono_ports_installs_default_for_both() {
        // 2 mono ports → port[0]=L, port[1]=R (dual-mono summed) — both routes
        // installed into the group.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "two_mono",
            vec![
                vibelang_dsp::OutputPort {
                    name: "L".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "R".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(3);
        let group = GroupId::new(1);
        handler
            .create(voice_id, VoiceConfig::new("v", "two_mono", group))
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(s.default_routes.len(), 2);
        assert_eq!(
            s.default_routes[&(voice_id, "L".to_string())],
            vec![crate::handlers::RouteDest::Group(group)],
        );
        assert_eq!(
            s.default_routes[&(voice_id, "R".to_string())],
            vec![crate::handlers::RouteDest::Group(group)],
        );
    }

    #[tokio::test]
    async fn create_voice_four_ports_only_first_two_default_routed() {
        // N>2 ports (e.g. spectraphon side) → only the first two get defaults;
        // the remaining ports stay un-routed (no entry = silent).
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "spectra_side",
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
                    name: "odd".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                vibelang_dsp::OutputPort {
                    name: "even".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(4);
        let group = GroupId::new(1);
        handler
            .create(voice_id, VoiceConfig::new("v", "spectra_side", group))
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(s.default_routes.len(), 2, "only first two ports defaulted");
        assert!(s
            .default_routes
            .contains_key(&(voice_id, "sine".to_string())));
        assert!(s
            .default_routes
            .contains_key(&(voice_id, "sub".to_string())));
        assert!(
            !s.default_routes
                .contains_key(&(voice_id, "odd".to_string())),
            "ports beyond the first two stay un-routed (silent)"
        );
        assert!(!s
            .default_routes
            .contains_key(&(voice_id, "even".to_string())));
    }

    #[tokio::test]
    async fn create_voice_does_not_overwrite_existing_default_routes() {
        // If a default entry already exists for `(voice_id, port_name)` (e.g.
        // a script-driven re-create path that didn't go through delete first),
        // the helper must NOT replace it — `entry().or_insert()` semantics.
        // Combined with the merge step in apply_reload, this guarantees
        // "explicit user route on port[0] not overridden by default."
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "stereo_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 2,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let voice_id = VoiceId::new(7);
        let voice_group = GroupId::new(1);
        let preset_dest = vec![crate::handlers::RouteDest::Main];
        {
            let mut s = state.write().await;
            s.default_routes
                .insert((voice_id, "out".to_string()), preset_dest.clone());
        }

        handler
            .create(voice_id, VoiceConfig::new("v", "stereo_synth", voice_group))
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(
            s.default_routes[&(voice_id, "out".to_string())],
            preset_dest,
            "create must not overwrite an existing default-routes entry"
        );
    }

    #[tokio::test]
    async fn delete_voice_drains_default_routes() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_synthdef_with_ports(
            &state,
            "mono_synth",
            vec![vibelang_dsp::OutputPort {
                name: "out".to_string(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;

        let voice_id = VoiceId::new(8);
        handler
            .create(
                voice_id,
                VoiceConfig::new("v", "mono_synth", GroupId::new(1)),
            )
            .await
            .unwrap();
        assert_eq!(state.read().await.default_routes.len(), 1);

        handler.delete(voice_id).await.unwrap();
        assert!(
            state.read().await.default_routes.is_empty(),
            "voice delete must drain its default routes from State"
        );
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

    #[tokio::test]
    async fn test_delete_voice_frees_note_nodes() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 64, 0.8).await.unwrap();

        handler.delete(voice_id).await.unwrap();

        assert_eq!(
            backend.nodes_freed(),
            2,
            "Held note nodes should be freed on delete"
        );
    }

    // =========================================================================
    // Node ID Reclamation Tests (CR-13)
    // =========================================================================

    #[tokio::test]
    async fn delete_voice_recycles_node_ids() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config.clone()).await.unwrap();
        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 64, 0.8).await.unwrap();
        let high_water = state.read().await.node_ids.allocated_count();

        handler.delete(voice_id).await.unwrap();
        handler.create(voice_id, config).await.unwrap();
        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 64, 0.8).await.unwrap();

        assert_eq!(
            state.read().await.node_ids.allocated_count(),
            high_water,
            "delete must recycle note-node IDs so recreate reuses them"
        );
    }

    #[tokio::test]
    async fn stop_voice_recycles_node_ids() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        let high_water = state.read().await.node_ids.allocated_count();

        handler.stop(voice_id).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        assert_eq!(
            state.read().await.node_ids.allocated_count(),
            high_water,
            "stop must recycle active-node IDs so later triggers reuse them"
        );
    }

    #[tokio::test]
    async fn trigger_polyphony_eviction_keeps_node_id_high_water_flat() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config =
            VoiceConfig::new("test_voice", "test_synth", GroupId::new(1)).with_polyphony(2);
        handler.create(voice_id, config).await.unwrap();

        for _ in 0..10 {
            handler.trigger(voice_id, &ParamMap::new()).await.unwrap();
        }

        // Steady state: polyphony sounding nodes + the one just allocated
        // before the eviction recycles the oldest.
        assert_eq!(
            state.read().await.node_ids.allocated_count(),
            3,
            "polyphony eviction must recycle node IDs (high-water = polyphony + 1)"
        );
    }

    #[tokio::test]
    async fn note_on_same_pitch_retrigger_recycles_node_id() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        for _ in 0..5 {
            handler.note_on(voice_id, 60, 0.8).await.unwrap();
        }

        assert_eq!(
            state.read().await.node_ids.allocated_count(),
            2,
            "same-pitch retrigger must recycle the replaced node's ID"
        );
    }

    #[tokio::test]
    async fn node_id_exhaustion_fails_operation_but_handler_survives() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        // Shrink the node-ID space to exactly two IDs.
        state.write().await.node_ids = crate::state::FreeListAllocator::new(1000, 1002);

        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 64, 0.8).await.unwrap();
        let err = handler.note_on(voice_id, 67, 0.8).await;
        assert!(
            matches!(err, Err(Error::IdsExhausted(_))),
            "exhausted allocator must surface as Err, got {:?}",
            err
        );

        // The failed note must not corrupt the handler: freeing IDs via stop
        // makes the next note succeed again.
        handler.stop(voice_id).await.unwrap();
        handler.note_on(voice_id, 67, 0.8).await.unwrap();
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
    async fn trigger_voice_with_input_route_adds_after_input_link() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let input_link_node = NodeId::new(1234);
        state.write().await.input_route_synths.insert(
            (
                voice_id,
                "carrier".to_string(),
                crate::handlers::InputRouteSrc::Silent,
            ),
            input_link_node,
        );

        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        let creates = backend.synth_create_calls();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].target, input_link_node);
        assert_eq!(creates[0].action, AddAction::After);
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
    async fn test_stop_voice_frees_note_nodes() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        handler.note_on(voice_id, 60, 0.8).await.unwrap();
        handler.note_on(voice_id, 64, 0.8).await.unwrap();

        handler.stop(voice_id).await.unwrap();

        assert_eq!(
            backend.nodes_freed(),
            2,
            "Held note nodes should be freed on stop"
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

    // =========================================================================
    // Unified poly(n) voice-allocation pool for MIDI-output voices
    // =========================================================================

    #[cfg(feature = "midi")]
    mod midi_pool {
        use super::*;
        use crate::midi::{QueuedMidiEvent, ScheduledMidiEvent};
        use crate::types::MidiDeviceId;
        use std::sync::Mutex;
        use std::time::Duration;

        const DEV: u8 = 7;

        /// Handler wired to a mock MIDI sink; returns the receiver that records
        /// every event the handler emits to the device.
        fn midi_handler() -> (
            VoicesHandler<MockBackend>,
            Arc<RwLock<State>>,
            crossbeam_channel::Receiver<ScheduledMidiEvent>,
        ) {
            let backend = Arc::new(MockBackend::new());
            let state = Arc::new(RwLock::new(State::default()));
            let mut handler = VoicesHandler::new(backend, state.clone());
            let (tx, rx) = crossbeam_channel::unbounded();
            let mut map = HashMap::new();
            map.insert(MidiDeviceId::new(DEV as u32), tx);
            handler.set_midi_outputs(Arc::new(Mutex::new(map)));
            (handler, state, rx)
        }

        async fn make_midi_voice(
            handler: &VoicesHandler<MockBackend>,
            state: &Arc<RwLock<State>>,
            polyphony: u8,
            legato: bool,
        ) -> VoiceId {
            setup_state_with_group(state).await;
            let voice_id = VoiceId::new(1);
            let config = VoiceConfig::new("mono_midi", "", GroupId::new(1))
                .with_midi_output(MidiDeviceId::new(DEV as u32), 0)
                .with_polyphony(polyphony)
                .with_mono_legato(legato);
            handler.create(voice_id, config).await.unwrap();
            voice_id
        }

        /// Drain the sink into a comparable `(kind, channel, note, velocity)` list.
        fn drained(
            rx: &crossbeam_channel::Receiver<ScheduledMidiEvent>,
        ) -> Vec<(&'static str, u8, u8, u8)> {
            let mut out = Vec::new();
            while let Ok(scheduled) = rx.try_recv() {
                out.push(match scheduled.event {
                    QueuedMidiEvent::NoteOn {
                        channel,
                        note,
                        velocity,
                    } => ("on", channel, note, velocity),
                    QueuedMidiEvent::NoteOff { channel, note } => ("off", channel, note, 0),
                    other => panic!("unexpected MIDI event: {:?}", other),
                });
            }
            out
        }

        fn drained_scheduled(
            rx: &crossbeam_channel::Receiver<ScheduledMidiEvent>,
        ) -> Vec<(Instant, &'static str, u8, u8, u8)> {
            let mut out = Vec::new();
            while let Ok(scheduled) = rx.try_recv() {
                let timestamp = scheduled.timestamp;
                out.push(match scheduled.event {
                    QueuedMidiEvent::NoteOn {
                        channel,
                        note,
                        velocity,
                    } => (timestamp, "on", channel, note, velocity),
                    QueuedMidiEvent::NoteOff { channel, note } => {
                        (timestamp, "off", channel, note, 0)
                    }
                    other => panic!("unexpected MIDI event: {:?}", other),
                });
            }
            out
        }

        #[tokio::test]
        async fn note_on_at_honors_future_timestamp() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;
            let target = Instant::now() + Duration::from_millis(20);

            handler.note_on_at(v, 60, 1.0, Some(target)).await.unwrap();

            let events = drained_scheduled(&rx);
            assert_eq!(events.len(), 1);
            assert_eq!(
                (events[0].1, events[0].2, events[0].3, events[0].4),
                ("on", 0, 60, 127)
            );
            assert!(events[0].0 >= target);
        }

        #[tokio::test]
        async fn note_off_at_honors_future_timestamp() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            let _ = drained(&rx);
            let target = Instant::now() + Duration::from_millis(20);
            handler.note_off_at(v, 60, Some(target)).await.unwrap();

            let events = drained_scheduled(&rx);
            assert_eq!(events.len(), 1);
            assert_eq!(
                (events[0].1, events[0].2, events[0].3, events[0].4),
                ("off", 0, 60, 0)
            );
            assert!(events[0].0 >= target);
        }

        #[tokio::test]
        async fn stale_delayed_same_pitch_note_off_does_not_release_retrigger() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            let stale_generation = handler.note_on_at_tracked(v, 60, 1.0, None).await.unwrap();
            let fresh_generation = handler.note_on_at_tracked(v, 60, 0.5, None).await.unwrap();

            assert_ne!(stale_generation, fresh_generation);
            assert_eq!(drained(&rx), vec![("on", 0, 60, 127), ("on", 0, 60, 63)]);

            handler
                .note_off_at_if_generation(v, 60, stale_generation, None)
                .await
                .unwrap();
            assert!(
                drained(&rx).is_empty(),
                "stale delayed note-off must not release the fresh same-pitch note"
            );

            handler
                .note_off_at_if_generation(v, 60, fresh_generation, None)
                .await
                .unwrap();
            assert_eq!(drained(&rx), vec![("off", 0, 60, 0)]);
        }

        /// Dispatching a full tick's batch of future-timestamped MIDI notes
        /// must return immediately (no inline sleeping on the runtime task)
        /// and enqueue every event with its exact scheduled deadline, with
        /// note-on before note-off per pitch.
        #[tokio::test]
        async fn batch_dispatch_of_future_notes_returns_without_sleeping() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 8, false).await;

            let start = Instant::now();
            let lookahead = Duration::from_millis(50);
            let base = start + lookahead;

            for i in 0..8u8 {
                let on = base + Duration::from_millis(i as u64);
                let off = base + Duration::from_millis(20 + i as u64);
                handler.note_on_at(v, 60 + i, 1.0, Some(on)).await.unwrap();
                handler.note_off_at(v, 60 + i, Some(off)).await.unwrap();
            }

            let elapsed = start.elapsed();
            assert!(
                elapsed < Duration::from_millis(25),
                "MIDI dispatch must not park the runtime task until the \
                 deadlines (lookahead {:?}): took {:?}",
                lookahead,
                elapsed
            );

            let events = drained_scheduled(&rx);
            assert_eq!(events.len(), 16, "8 note-ons + 8 note-offs expected");
            for i in 0..8u64 {
                let note = 60 + i as u8;
                let on = events
                    .iter()
                    .find(|e| e.1 == "on" && e.3 == note)
                    .unwrap_or_else(|| panic!("missing note-on for {}", note));
                let off = events
                    .iter()
                    .find(|e| e.1 == "off" && e.3 == note)
                    .unwrap_or_else(|| panic!("missing note-off for {}", note));
                assert_eq!(on.0, base + Duration::from_millis(i));
                assert_eq!(off.0, base + Duration::from_millis(20 + i));
                assert!(on.0 < off.0, "note-on must precede note-off");
            }
        }

        /// A mono-pool steal scheduled for a single future deadline emits
        /// NoteOn(A), NoteOff(A), NoteOn(B) — and that order must survive
        /// the output thread's timestamp heap (creation-seq tie-break),
        /// so B is not killed by A's release.
        #[tokio::test]
        async fn same_deadline_steal_keeps_order_through_output_heap() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;
            let deadline = Instant::now() + Duration::from_millis(30);

            handler
                .note_on_at(v, 60, 1.0, Some(deadline))
                .await
                .unwrap();
            handler
                .note_on_at(v, 64, 0.5, Some(deadline))
                .await
                .unwrap(); // steals 60

            // Replay the enqueued events through the same structure the
            // output thread uses.
            let mut heap = std::collections::BinaryHeap::new();
            while let Ok(scheduled) = rx.try_recv() {
                assert_eq!(scheduled.timestamp, deadline);
                heap.push(scheduled);
            }

            let order: Vec<(&'static str, u8)> = std::iter::from_fn(|| heap.pop())
                .map(|s| match s.event {
                    QueuedMidiEvent::NoteOn { note, .. } => ("on", note),
                    QueuedMidiEvent::NoteOff { note, .. } => ("off", note),
                    other => panic!("unexpected MIDI event: {:?}", other),
                })
                .collect();

            assert_eq!(order, vec![("on", 60), ("off", 60), ("on", 64)]);
        }

        // Acceptance #1: overlapping stream A on, B on, A off ⇒
        // NoteOn A, NoteOff A, NoteOn B (B retriggers cleanly, survives A release).
        #[tokio::test]
        async fn steal_then_release_stolen_note() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A on (vel 127)
            handler.note_on(v, 64, 0.5).await.unwrap(); // B on (vel 63)
            handler.note_off(v, 60).await.unwrap(); // A off — A was stolen, nothing emitted

            assert_eq!(
                drained(&rx),
                vec![("on", 0, 60, 127), ("off", 0, 60, 0), ("on", 0, 64, 63)],
            );
        }

        // Acceptance #2: releasing the top note with an earlier note still held
        // returns to that held note (re-NoteOn with its stored velocity).
        #[tokio::test]
        async fn release_top_returns_to_held() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A on
            handler.note_on(v, 64, 0.5).await.unwrap(); // B on (steals A)
            let _ = drained(&rx);
            handler.note_off(v, 64).await.unwrap(); // B off — NoteOff B, then return to A at vel 127

            assert_eq!(drained(&rx), vec![("off", 0, 64, 0), ("on", 0, 60, 127)]);
        }

        // Acceptance #3: releasing the last held note sends NoteOff.
        #[tokio::test]
        async fn release_last_note_sends_note_off() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            let _ = drained(&rx);
            handler.note_off(v, 60).await.unwrap();

            assert_eq!(drained(&rx), vec![("off", 0, 60, 0)]);
        }

        // No regression: a poly(n>1) MIDI-output voice fed fewer notes than it
        // has slots never steals, so every note-on/off passes straight through.
        #[tokio::test]
        async fn polyphonic_midi_voice_passes_through_without_steal() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 4, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            handler.note_on(v, 64, 1.0).await.unwrap();
            handler.note_off(v, 60).await.unwrap();
            handler.note_off(v, 64).await.unwrap();

            assert_eq!(
                drained(&rx),
                vec![
                    ("on", 0, 60, 127),
                    ("on", 0, 64, 127),
                    ("off", 0, 60, 0),
                    ("off", 0, 64, 0),
                ],
            );
        }

        // .mono_legato(true) skips the pre-NoteOff so portamento synths slur.
        #[tokio::test]
        async fn legato_skips_pre_note_off() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, true).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            handler.note_on(v, 64, 1.0).await.unwrap(); // no NoteOff(60) first

            assert_eq!(drained(&rx), vec![("on", 0, 60, 127), ("on", 0, 64, 127)]);
        }

        // Voice stop releases the still-sounding note and clears the pool.
        #[tokio::test]
        async fn stop_releases_sounding_note() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            handler.note_on(v, 64, 1.0).await.unwrap();
            let _ = drained(&rx);
            handler.stop(v).await.unwrap();

            assert_eq!(drained(&rx), vec![("off", 0, 64, 0)]);
            assert!(state.read().await.midi_voice_pool.is_empty());
        }

        // =====================================================================
        // poly(n > 1) scenarios
        // =====================================================================

        // poly(3) fed 4 overlapping note-ons (A, B, C, D): A/B/C take free
        // slots; D steals the oldest (A) → NoteOff A + NoteOn D, A to overflow.
        // Releasing D revives A — and because this is a poly destination (n > 1)
        // the released note D gets an explicit NoteOff first (nothing implicitly
        // cuts it the way a NoteOn does on a mono synth), so D can't stick.
        // Releasing the remaining 3 sends 3 NoteOffs.
        #[tokio::test]
        async fn poly3_steal_revive_and_drain() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 3, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A
            handler.note_on(v, 62, 1.0).await.unwrap(); // B
            handler.note_on(v, 64, 1.0).await.unwrap(); // C
            handler.note_on(v, 65, 1.0).await.unwrap(); // D — steals A
            assert_eq!(
                drained(&rx),
                vec![
                    ("on", 0, 60, 127),
                    ("on", 0, 62, 127),
                    ("on", 0, 64, 127),
                    ("off", 0, 60, 0),
                    ("on", 0, 65, 127),
                ],
            );

            handler.note_off(v, 65).await.unwrap(); // D off — NoteOff D, then revive A
            assert_eq!(drained(&rx), vec![("off", 0, 65, 0), ("on", 0, 60, 127)]);

            handler.note_off(v, 62).await.unwrap();
            handler.note_off(v, 64).await.unwrap();
            handler.note_off(v, 60).await.unwrap();
            assert_eq!(
                drained(&rx),
                vec![("off", 0, 62, 0), ("off", 0, 64, 0), ("off", 0, 60, 0)],
            );
            assert!(state.read().await.midi_voice_pool.is_empty());
        }

        // poly(3): two notes, release one, then a new one — the new note takes
        // the freed slot, no steal, no NoteOff-on-steal.
        #[tokio::test]
        async fn poly3_free_slot_no_steal() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 3, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A
            handler.note_on(v, 62, 1.0).await.unwrap(); // B
            handler.note_off(v, 60).await.unwrap(); // A off
            handler.note_on(v, 64, 1.0).await.unwrap(); // C — free slot, no steal

            assert_eq!(
                drained(&rx),
                vec![
                    ("on", 0, 60, 127),
                    ("on", 0, 62, 127),
                    ("off", 0, 60, 0),
                    ("on", 0, 64, 127),
                ],
            );
        }

        // Re-pressing an already-sounding note retriggers it (NoteOn re-sent
        // with the new velocity) without stealing any slot.
        #[tokio::test]
        async fn poly3_retrigger_held_note() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 3, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A vel 127
            handler.note_on(v, 62, 1.0).await.unwrap(); // B
            handler.note_on(v, 60, 0.5).await.unwrap(); // A again vel 63 — retrigger

            assert_eq!(
                drained(&rx),
                vec![("on", 0, 60, 127), ("on", 0, 62, 127), ("on", 0, 60, 63)],
            );
            // Still only two slots occupied — no steal happened.
            assert_eq!(state.read().await.midi_voice_pool[&v].overflow.len(), 0);
        }

        // mono_legato(true) with n > 1: a steal still skips the pre-NoteOff.
        #[tokio::test]
        async fn poly2_legato_skips_pre_note_off_on_steal() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 2, true).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A
            handler.note_on(v, 62, 1.0).await.unwrap(); // B
            handler.note_on(v, 64, 1.0).await.unwrap(); // C — steals A, no NoteOff(A)

            assert_eq!(
                drained(&rx),
                vec![("on", 0, 60, 127), ("on", 0, 62, 127), ("on", 0, 64, 127)],
            );
        }

        // Reload shrinking polyphony 3 → 1: the two oldest sounding notes get
        // NoteOff, pool shrinks to one slot keeping the most-recent note.
        #[tokio::test]
        async fn reload_shrink_polyphony_note_offs_excess() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 3, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap(); // A (oldest)
            handler.note_on(v, 62, 1.0).await.unwrap(); // B
            handler.note_on(v, 64, 1.0).await.unwrap(); // C (newest)
            let _ = drained(&rx);

            handler.resize_midi_pool(v, 1).await.unwrap();
            assert_eq!(drained(&rx), vec![("off", 0, 60, 0), ("off", 0, 62, 0)]);
            {
                let s = state.read().await;
                let pool = &s.midi_voice_pool[&v];
                assert_eq!(pool.slots.len(), 1);
                assert_eq!(pool.slots[0], Some((64, 127)));
            }

            // The surviving note still releases cleanly afterwards.
            handler.note_off(v, 64).await.unwrap();
            assert_eq!(drained(&rx), vec![("off", 0, 64, 0)]);
        }

        // Reload growing polyphony 1 → 3: pool grows, no spurious MIDI events.
        #[tokio::test]
        async fn reload_grow_polyphony_no_events() {
            let (handler, state, rx) = midi_handler();
            let v = make_midi_voice(&handler, &state, 1, false).await;

            handler.note_on(v, 60, 1.0).await.unwrap();
            handler.note_on(v, 64, 1.0).await.unwrap(); // steals 60 → overflow
            let _ = drained(&rx);

            handler.resize_midi_pool(v, 3).await.unwrap();
            assert!(drained(&rx).is_empty());
            {
                let s = state.read().await;
                let pool = &s.midi_voice_pool[&v];
                assert_eq!(pool.slots.len(), 3);
                assert_eq!(pool.slots[0], Some((64, 127)));
            }
        }
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

    // =========================================================================
    // Multi-output bus allocation tests (Story 2)
    // =========================================================================

    use vibelang_dsp::OutputPort;

    /// Helper: register a synthdef with an explicit port set on the state.
    async fn register_multiport_synthdef(
        state: &Arc<RwLock<State>>,
        name: &str,
        ports: Vec<OutputPort>,
    ) {
        let mut state_write = state.write().await;
        state_write.synthdefs.insert(name.to_string());
        state_write.synthdef_outputs.insert(name.to_string(), ports);
    }

    fn register_synthdef_param_defaults(name: &str, params: &[(&str, f32)]) {
        let ir = vibelang_dsp::GraphIR {
            name: name.to_string(),
            constants: Vec::new(),
            params: params
                .iter()
                .enumerate()
                .map(|(index, (name, value))| vibelang_dsp::ParamSpec {
                    name: (*name).to_string(),
                    default: vec![*value],
                    index,
                    lag_ms: None,
                })
                .collect(),
            nodes: Vec::new(),
            out_bus: 0,
        };
        vibelang_dsp::register_synthdef_ir(name.to_string(), ir);
    }

    #[tokio::test]
    async fn test_legacy_voice_owns_one_stereo_bus_pair() {
        // Synthdef with no explicit port set falls back to the legacy
        // [("out", 2)] default — voice should own exactly one stereo pair.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("legacy_voice", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert_eq!(
            voice.output_buses.len(),
            1,
            "legacy voice owns exactly one bus chunk"
        );
        assert_eq!(voice.output_buses[0].0, "out");

        // Allocator should have advanced by 2 (one stereo pair).
        assert_eq!(state_read.audio_buses.allocated_count(), 2);
    }

    #[tokio::test]
    async fn test_multiport_voice_owns_distinct_bus_ranges() {
        // 4-port synthdef: mono, mono, stereo, mono — total 5 bus IDs.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "quad_synth",
            vec![
                OutputPort {
                    name: "a".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "b".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "c".into(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "d".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "quad_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();

        // 4 ports → 4 entries, in declared order.
        assert_eq!(voice.output_buses.len(), 4);
        let names: Vec<&str> = voice.output_buses.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c", "d"]);

        // Each port owns a distinct starting bus id.
        let bus_ids: Vec<u32> = voice.output_buses.iter().map(|(_, b)| b.raw()).collect();
        let mut sorted = bus_ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "all four port buses must be distinct");

        // Widths match: total advance is 1+1+2+1 = 5 IDs from the bus floor.
        assert_eq!(state_read.audio_buses.allocated_count(), 5);

        // The stereo port's chunk must not collide with the next mono chunk.
        // Buses are carved sequentially in declaration order, so:
        //   a = 16, b = 17, c = 18 (occupies 18,19), d = 20.
        let a = bus_ids[0];
        assert_eq!(bus_ids[1], a + 1, "b follows a");
        assert_eq!(bus_ids[2], a + 2, "c follows b");
        assert_eq!(bus_ids[3], a + 4, "d follows c+1 (stereo skip)");
    }

    #[tokio::test]
    async fn test_explicit_ports_route_to_owned_buses() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "owned_explicit_synth",
            vec![
                OutputPort {
                    name: "left".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "right".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;
        register_synthdef_param_defaults("owned_explicit_synth", &[("out0", 0.0), ("out1", 0.0)]);

        let voice_id = VoiceId::new(1);
        handler
            .create(
                voice_id,
                VoiceConfig::new("owned_explicit", "owned_explicit_synth", GroupId::new(1)),
            )
            .await
            .unwrap();

        let output_buses = {
            let state_read = state.read().await;
            state_read
                .voices
                .get(&voice_id)
                .unwrap()
                .output_buses
                .iter()
                .map(|(_, bus)| bus.raw())
                .collect::<Vec<_>>()
        };

        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        let creates = backend.synth_create_params();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].get("out0"), Some(&(output_buses[0] as f32)));
        assert_eq!(creates[0].get("out1"), Some(&(output_buses[1] as f32)));
        assert_eq!(creates[0].get("out"), Some(&16.0));
    }

    #[tokio::test]
    async fn test_user_out_param_wins_over_owned_bus_allocation() {
        let (handler, backend, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "single_owned_out_synth",
            vec![OutputPort {
                name: "signal".into(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Ar,
            }],
        )
        .await;
        register_synthdef_param_defaults("single_owned_out_synth", &[("out", 0.0)]);

        let voice_id = VoiceId::new(1);
        let mut config = VoiceConfig::new(
            "single_owned_out",
            "single_owned_out_synth",
            GroupId::new(1),
        );
        config.params.insert("out".to_string(), 99.0);
        handler.create(voice_id, config).await.unwrap();

        handler.trigger(voice_id, &ParamMap::new()).await.unwrap();

        let creates = backend.synth_create_params();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].get("out"), Some(&99.0));
    }

    #[tokio::test]
    async fn test_delete_voice_frees_all_owned_buses() {
        // After a delete, every owned chunk is back in the free list with the
        // matching width, so a re-create of the same synthdef reuses them.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "quad_synth",
            vec![
                OutputPort {
                    name: "a".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "b".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "c".into(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "d".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "quad_synth", GroupId::new(1));
        handler.create(voice_id, config.clone()).await.unwrap();

        let buses_before: Vec<u32> = {
            let state_read = state.read().await;
            state_read
                .voices
                .get(&voice_id)
                .unwrap()
                .output_buses
                .iter()
                .map(|(_, b)| b.raw())
                .collect()
        };
        assert_eq!(buses_before.len(), 4);

        handler.delete(voice_id).await.unwrap();

        // Allocator counter does NOT advance on free — the four chunks are
        // back in the free list. A re-creation hands them out in FIFO order
        // for matching widths.
        let allocated_after_delete = {
            let state_read = state.read().await;
            state_read.audio_buses.allocated_count()
        };
        assert_eq!(
            allocated_after_delete, 5,
            "free does not advance the monotonic counter"
        );

        let voice2 = VoiceId::new(2);
        handler.create(voice2, config).await.unwrap();

        let state_read = state.read().await;
        let buses_after: Vec<u32> = state_read
            .voices
            .get(&voice2)
            .unwrap()
            .output_buses
            .iter()
            .map(|(_, b)| b.raw())
            .collect();

        // Counter still at 5 — every port reused a freed chunk of matching width.
        assert_eq!(state_read.audio_buses.allocated_count(), 5);
        // The reused bus set must equal the original (any order — different
        // widths come from independent FIFO sub-pools).
        let mut a = buses_before;
        a.sort();
        let mut b = buses_after;
        b.sort();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn test_graceful_delete_voice_frees_buses() {
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("legacy", "test_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        handler.graceful_delete(voice_id).await.unwrap();

        {
            let state_read = state.read().await;
            assert!(!state_read.voices.contains_key(&voice_id));
        }

        // Stereo pair returned to the pool — a new stereo alloc reuses bus 16.
        let mut state_write = state.write().await;
        let bus = state_write.alloc_audio_bus(2).unwrap();
        assert_eq!(bus.raw(), 16, "freed stereo pair must be reused");
    }

    // =========================================================================
    // Multi-output v2 Story 2: control-bus alloc per kr port
    // =========================================================================

    #[tokio::test]
    async fn test_mixed_rate_voice_alloc_audio_and_control() {
        // 4-port synthdef with rates [Ar, Ar, Kr, Kr]: two audio mono ports
        // and two control ports. Each Ar port consumes one audio bus ID; each
        // Kr port consumes one control bus ID — segregated free lists.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "mixed_synth",
            vec![
                OutputPort {
                    name: "l".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "r".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "env".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                },
                OutputPort {
                    name: "lfo".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "mixed_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert_eq!(voice.output_buses.len(), 4);
        let names: Vec<&str> = voice.output_buses.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["l", "r", "env", "lfo"]);

        // 2 Ar ports of width 1 → 2 audio bus IDs carved from the audio pool.
        assert_eq!(
            state_read.audio_buses.allocated_count(),
            2,
            "two Ar ports advance the audio counter by 2",
        );
        // 2 Kr ports → 2 control bus IDs carved from the control pool.
        assert_eq!(
            state_read.control_buses.allocated_count(),
            2,
            "two Kr ports advance the control counter by 2",
        );

        // Pools are segregated: control IDs start at 1000, audio at 16.
        let l = voice.output_buses[0].1.raw();
        let r = voice.output_buses[1].1.raw();
        let env = voice.output_buses[2].1.raw();
        let lfo = voice.output_buses[3].1.raw();
        assert!(l < 1000 && r < 1000, "Ar ports own audio-bus IDs");
        assert!(env >= 1000 && lfo >= 1000, "Kr ports own control-bus IDs");
        assert_ne!(env, lfo, "two Kr ports get distinct control IDs");
    }

    #[tokio::test]
    async fn test_mixed_rate_voice_drop_frees_both_kinds() {
        // After a delete, both the audio chunks and the control buses are
        // back in their respective pools — re-creating the same voice reuses
        // them, so neither counter advances further.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "mixed_synth",
            vec![
                OutputPort {
                    name: "l".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "r".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                },
                OutputPort {
                    name: "env".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                },
                OutputPort {
                    name: "lfo".into(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                },
            ],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "mixed_synth", GroupId::new(1));
        handler.create(voice_id, config.clone()).await.unwrap();

        let buses_before: Vec<u32> = {
            let s = state.read().await;
            s.voices
                .get(&voice_id)
                .unwrap()
                .output_buses
                .iter()
                .map(|(_, b)| b.raw())
                .collect()
        };

        handler.delete(voice_id).await.unwrap();

        // Counters do not advance on free.
        {
            let s = state.read().await;
            assert_eq!(s.audio_buses.allocated_count(), 2);
            assert_eq!(s.control_buses.allocated_count(), 2);
        }

        // Re-create — every port reuses a freed ID from the matching pool.
        let voice2 = VoiceId::new(2);
        handler.create(voice2, config).await.unwrap();

        let s = state.read().await;
        assert_eq!(s.audio_buses.allocated_count(), 2, "audio pool reused");
        assert_eq!(s.control_buses.allocated_count(), 2, "control pool reused");

        let buses_after: Vec<u32> = s
            .voices
            .get(&voice2)
            .unwrap()
            .output_buses
            .iter()
            .map(|(_, b)| b.raw())
            .collect();
        let mut a = buses_before;
        a.sort();
        let mut b = buses_after;
        b.sort();
        assert_eq!(a, b, "freed bus set must equal reused bus set");
    }

    // =========================================================================
    // B2.b: Tr port voice bus alloc — shares the control-bus path with kr.
    // =========================================================================

    #[tokio::test]
    async fn test_voice_with_one_tr_port_owns_one_control_bus() {
        // A 1-port tr synthdef carves exactly one ID from the control-bus
        // pool — no audio-bus advance beyond the group's stereo pair.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "trig_synth",
            vec![OutputPort {
                name: "trig".into(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Tr,
            }],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "trig_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let state_read = state.read().await;
        let voice = state_read.voices.get(&voice_id).unwrap();
        assert_eq!(voice.output_buses.len(), 1);
        assert_eq!(voice.output_buses[0].0, "trig");

        let bus = voice.output_buses[0].1.raw();
        assert!(bus >= 1000, "tr port owns a control-bus id (got {})", bus);
        assert_eq!(
            state_read.control_buses.allocated_count(),
            1,
            "one tr port advances the control-bus counter by 1",
        );
        // Audio-bus counter is untouched — the test group hard-wires its
        // audio_bus rather than allocating, and the tr port lives on the
        // control-bus pool.
        assert_eq!(state_read.audio_buses.allocated_count(), 0);
    }

    #[tokio::test]
    async fn test_voice_drop_frees_tr_port_control_bus() {
        // After `delete`, the tr port's control bus returns to the pool,
        // so the next control-bus alloc hands it back.
        let (handler, _, state) = create_handler_with_group();
        setup_state_with_group(&state).await;
        register_multiport_synthdef(
            &state,
            "trig_synth",
            vec![OutputPort {
                name: "trig".into(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Tr,
            }],
        )
        .await;

        let voice_id = VoiceId::new(1);
        let config = VoiceConfig::new("voice", "trig_synth", GroupId::new(1));
        handler.create(voice_id, config).await.unwrap();

        let trig_bus = {
            let s = state.read().await;
            s.voices
                .get(&voice_id)
                .unwrap()
                .output_buses
                .iter()
                .find(|(n, _)| n == "trig")
                .map(|(_, b)| b.raw())
                .unwrap()
        };

        handler.delete(voice_id).await.unwrap();

        // Control-bus counter does NOT advance on free.
        {
            let s = state.read().await;
            assert_eq!(s.control_buses.allocated_count(), 1);
        }

        // Next alloc reuses the freed id.
        let reused = {
            let mut s = state.write().await;
            s.alloc_control_bus().unwrap()
        };
        assert_eq!(
            reused.raw(),
            trig_bus,
            "freed tr control-bus reused on next alloc"
        );
    }

    #[tokio::test]
    async fn test_alloc_free_alloc_reuse_for_both_pools() {
        // Direct hammer test for the State wrappers — alloc, free, alloc must
        // hand back the same ID for each pool, independently.
        let state = Arc::new(RwLock::new(State::default()));
        let mut s = state.write().await;

        // Audio pool: alloc → free → alloc returns the same starting ID.
        let a1 = s.alloc_audio_bus(1).unwrap();
        s.free_audio_bus(a1, 1);
        let a2 = s.alloc_audio_bus(1).unwrap();
        assert_eq!(a1.raw(), a2.raw(), "audio bus is reused after free");

        // Control pool: alloc → free → alloc returns the same ID.
        let c1 = s.alloc_control_bus().unwrap();
        s.free_control_bus(c1);
        let c2 = s.alloc_control_bus().unwrap();
        assert_eq!(c1.raw(), c2.raw(), "control bus is reused after free");

        // The two pools live in disjoint ranges (audio < 1000 ≤ control).
        assert!(a2.raw() < 1000);
        assert!(c2.raw() >= 1000);
    }
}

#[cfg(all(test, feature = "midi"))]
mod midi_pool_tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    /// A note event: `(is_note_on, note)`.
    type Ev = (bool, u8);

    /// Drive a sequence of note events through the unified pool for one voice
    /// with `n` slots and return the flat list of device events emitted
    /// (`(is_note_on, note)`) in order.
    fn drive(n: usize, legato: bool, events: &[Ev]) -> Vec<Ev> {
        let mut state = State::default();
        let id = VoiceId::new(1);
        let channel = 0u8;
        let mut out = Vec::new();
        for &(on, note) in events {
            let emitted = if on {
                midi_pool_note_on(&mut state, id, channel, note, 64, n, legato)
            } else {
                midi_pool_note_off(&mut state, id, channel, note, n)
            };
            for ev in emitted {
                match ev {
                    QueuedMidiEvent::NoteOn { note, .. } => out.push((true, note)),
                    QueuedMidiEvent::NoteOff { note, .. } => out.push((false, note)),
                    other => panic!("pool emitted unexpected event: {other:?}"),
                }
            }
        }
        out
    }

    /// Replay the emitted device events and assert that, once every key is
    /// released, no note is left sounding on the destination. A mono
    /// destination (`n == 1`) implicitly cuts the previous note on each NoteOn;
    /// a poly destination tracks each note independently. A NoteOff for a note
    /// that isn't currently sounding (a defensive flush) is a harmless no-op.
    fn assert_nothing_sounding(n: usize, emitted: &[Ev]) {
        if n == 1 {
            let mut sounding: Option<u8> = None;
            for &(on, note) in emitted {
                if on {
                    sounding = Some(note);
                } else if sounding == Some(note) {
                    sounding = None;
                }
            }
            assert!(
                sounding.is_none(),
                "mono voice left note {sounding:?} sounding (events: {emitted:?})"
            );
        } else {
            let mut sounding: HashSet<u8> = HashSet::new();
            for &(on, note) in emitted {
                if on {
                    sounding.insert(note);
                } else {
                    sounding.remove(&note);
                }
            }
            assert!(
                sounding.is_empty(),
                "poly voice left notes {sounding:?} sounding (events: {emitted:?})"
            );
        }
    }

    /// Net (NoteOn − NoteOff) per note over the emitted events.
    fn net_per_note(emitted: &[Ev]) -> HashMap<u8, i32> {
        let mut net: HashMap<u8, i32> = HashMap::new();
        for &(on, note) in emitted {
            *net.entry(note).or_default() += if on { 1 } else { -1 };
        }
        net
    }

    #[test]
    fn mono_simple_press_release_is_balanced() {
        let emitted = drive(1, false, &[(true, 60), (false, 60)]);
        assert_eq!(emitted, vec![(true, 60), (false, 60)]);
        assert_nothing_sounding(1, &emitted);
        assert!(net_per_note(&emitted).values().all(|&c| c == 0));
    }

    #[test]
    fn mono_steal_and_revive() {
        // Press A, press B (steals A → NoteOff A, NoteOn B), release B → NoteOff B
        // then revive A, release A → NoteOff A. Every note that got a NoteOn gets
        // a matching NoteOff — a mono synth's note-priority stack must not be left
        // thinking B is still held (otherwise releasing A sticks B's gate open).
        let emitted = drive(
            1,
            false,
            &[(true, 60), (true, 64), (false, 64), (false, 60)],
        );
        assert_eq!(
            emitted,
            vec![
                (true, 60),
                (false, 60),
                (true, 64),
                (false, 64),
                (true, 60),
                (false, 60)
            ],
        );
        assert_nothing_sounding(1, &emitted);
        assert!(net_per_note(&emitted).values().all(|&c| c == 0));
    }

    #[test]
    fn mono_release_stolen_note_out_of_order() {
        // Press A, press B (steal A), release A first (the stolen one — must not
        // resurrect or leak), release B → NoteOff B.
        let emitted = drive(
            1,
            false,
            &[(true, 60), (true, 64), (false, 60), (false, 64)],
        );
        assert_nothing_sounding(1, &emitted);
        assert!(net_per_note(&emitted).values().all(|&c| c == 0));
    }

    #[test]
    fn mono_repress_stolen_note() {
        // Press A, press B (steals A), re-press A (steals B), release A → revive B,
        // release B → NoteOff B.
        let emitted = drive(
            1,
            false,
            &[(true, 60), (true, 64), (true, 60), (false, 60), (false, 64)],
        );
        assert_nothing_sounding(1, &emitted);
        assert!(net_per_note(&emitted).values().all(|&c| c == 0));
    }

    #[test]
    fn mono_deep_steal_chain_unwinds_cleanly() {
        // Four notes held in turn on a mono voice, released in reverse — every
        // release revives the previous held note; the last is a NoteOff.
        let emitted = drive(
            1,
            false,
            &[
                (true, 60),
                (true, 62),
                (true, 64),
                (true, 65),
                (false, 65),
                (false, 64),
                (false, 62),
                (false, 60),
            ],
        );
        assert_nothing_sounding(1, &emitted);
        assert!(net_per_note(&emitted).values().all(|&c| c == 0));
    }

    #[test]
    fn poly3_overflow_and_release_interleavings() {
        // Fill 3 slots, a 4th steals the oldest (60 → NoteOff 60, overflow).
        // Release a held note → poly destination gets an explicit NoteOff for it
        // plus the revive's NoteOn for 60. Everything balances to zero.
        let emitted = drive(
            3,
            false,
            &[
                (true, 60),
                (true, 62),
                (true, 64),
                (true, 65),
                (false, 62),
                (false, 65),
                (false, 64),
                (false, 60),
            ],
        );
        assert_nothing_sounding(3, &emitted);
        assert!(
            net_per_note(&emitted).values().all(|&c| c == 0),
            "unbalanced NoteOn/NoteOff on poly(3): {emitted:?}"
        );
    }

    #[test]
    fn poly3_chord_balanced() {
        let emitted = drive(
            3,
            false,
            &[
                (true, 60),
                (true, 64),
                (true, 67),
                (false, 64),
                (false, 67),
                (false, 60),
            ],
        );
        assert_eq!(
            emitted,
            vec![
                (true, 60),
                (true, 64),
                (true, 67),
                (false, 64),
                (false, 67),
                (false, 60),
            ]
        );
        assert_nothing_sounding(3, &emitted);
    }

    #[test]
    fn release_untracked_note_emits_defensive_note_off() {
        // No prior NoteOn for note 47 — releasing it still flushes a NoteOff so a
        // bookkeeping desync cannot leave the device's gate stuck open.
        let emitted = drive(1, false, &[(false, 47)]);
        assert_eq!(emitted, vec![(false, 47)]);
        assert_nothing_sounding(1, &emitted);
    }

    #[test]
    fn release_after_pool_cleared_mid_hold_emits_note_off() {
        // Press A on a mono voice, simulate a reload clearing the pool (which
        // flushes a NoteOff for the sounding slot), then the delayed release
        // arrives → defensive NoteOff for A.
        let mut state = State::default();
        let id = VoiceId::new(1);
        let on = midi_pool_note_on(&mut state, id, 0, 60, 64, 1, false);
        assert!(matches!(
            on.as_slice(),
            [QueuedMidiEvent::NoteOn { note: 60, .. }]
        ));
        let cleared = midi_pool_clear(&mut state, id, 0);
        assert!(matches!(
            cleared.as_slice(),
            [QueuedMidiEvent::NoteOff { note: 60, .. }]
        ));
        let off = midi_pool_note_off(&mut state, id, 0, 60, 1);
        assert!(matches!(
            off.as_slice(),
            [QueuedMidiEvent::NoteOff { note: 60, .. }]
        ));
        assert!(!state.midi_voice_pool.contains_key(&id));
    }

    #[test]
    fn legato_steal_skips_note_off_but_release_still_clears() {
        // With legato, stealing does not emit NoteOff(stolen) (portamento slur),
        // but the eventual releases must still leave nothing sounding.
        let emitted = drive(1, true, &[(true, 60), (true, 64), (false, 64), (false, 60)]);
        assert_nothing_sounding(1, &emitted);
    }
}
