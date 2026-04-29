//! Runtime state management.
//!
//! The [`State`] struct holds all runtime state, including:
//!
//! - Transport state (tempo, position, playing)
//! - Loaded resources (synthdefs, samples)
//! - Entities (groups, voices, patterns, melodies, sequences, effects)
//! - Active playback state (running synths, active fades)

use crate::compat::Instant;
use crate::reload::ChangeQuant;
#[cfg(not(target_arch = "wasm32"))]
use crate::traits::RecordingInfo;
use crate::traits::{
    BufferConfig, FadeConfig, MelodyConfig, MelodyContent, ModulatorConfig, PatternConfig,
    PatternContent, SampleInfo, SequenceConfig, VoiceConfig,
};
use crate::types::FadeId;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::RecordingId;
use crate::types::{
    Beat, BufferId, BusId, ControlBusId, EffectId, GroupId, MelodyId, ModulatorId, NodeId,
    ParamMap, PatternId, SampleId, SequenceId, SfzId, TimeSignature, VoiceId,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use vibelang_dsp::OutputPort;

/// Internal state for a group.
#[derive(Clone, Debug)]
pub struct GroupState {
    /// Group ID.
    pub id: GroupId,

    /// Group name (for display and API lookup).
    pub name: String,

    /// Parent group (None for root).
    pub parent: Option<GroupId>,

    /// SuperCollider node ID for this group.
    pub node_id: NodeId,

    /// Audio bus ID for this group.
    ///
    /// Voices in this group output to this bus.
    /// The link synth reads from this bus and writes to the parent's bus (or bus 0 for main output).
    pub audio_bus: BusId,

    /// Node ID of the link synth that routes audio from this group's bus to the parent.
    ///
    /// Created by `FinalizeGroups` after all groups are set up.
    /// Uses the `system_link_audio` synthdef.
    pub link_synth_node_id: Option<NodeId>,

    /// Whether the group is muted.
    pub muted: bool,

    /// Whether the group is soloed.
    pub soloed: bool,

    /// Current parameter values (applied to link synth).
    pub params: ParamMap,

    /// Hardware output bus override (0-indexed channel number).
    /// When set, the link synth routes directly to this hardware bus
    /// instead of mixing into the parent group.
    pub output_bus: Option<u32>,
}

/// Internal state for a voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    /// Voice ID.
    pub id: VoiceId,

    /// Configuration.
    pub config: VoiceConfig,

    /// Active synth node IDs (for polyphony).
    pub active_nodes: Vec<NodeId>,

    /// Note -> node ID mapping.
    pub note_nodes: HashMap<u8, NodeId>,

    /// Round-robin counter for cycling through sample variations.
    ///
    /// Incremented on each trigger, wraps around based on the configured
    /// round-robin count. Useful for drum patterns with multiple sample
    /// variations (e.g., snare_1, snare_2, snare_3).
    pub round_robin_position: u32,

    /// Pending parameter changes from MIDI routing.
    ///
    /// These are applied when the next note triggers.
    pub pending_params: HashMap<String, f64>,

    /// Audio buses owned by this voice, one per declared output port.
    ///
    /// Populated at voice creation from the synthdef's `OutputPort` set.
    /// Each entry maps a port name to the starting `BusId` of a chunk
    /// `channels` wide (allocated via [`State::alloc_audio_bus`]).
    /// Freed at voice drop with the matching channel count.
    pub output_buses: Vec<(String, BusId)>,
}

/// Internal state for a pattern.
///
/// Separates immutable content (steps, voice, length) from mutable playback state
/// (playing, loop_position) to enable seamless hot reload. Content can be swapped
/// atomically at musical boundaries without affecting playback continuity.
#[derive(Clone, Debug)]
pub struct PatternState {
    /// Pattern ID.
    pub id: PatternId,

    /// The pattern content (steps, voice, length, etc.).
    /// Wrapped in Arc for cheap cloning and atomic swapping during hot reload.
    pub content: Arc<PatternContent>,

    /// Whether the pattern is playing.
    pub playing: bool,

    /// Current loop position.
    pub loop_position: Beat,

    /// Pending content swap for hot reload.
    /// When set, the new content will be applied at the specified quantization boundary.
    pub pending_content: Option<(Arc<PatternContent>, ChangeQuant)>,
}

impl PatternState {
    /// Create a new pattern state from a config.
    pub fn new(id: PatternId, config: PatternConfig) -> Self {
        Self {
            id,
            content: PatternContent::arc_from_config(&config),
            playing: false,
            loop_position: Beat::ZERO,
            pending_content: None,
        }
    }

    /// Get the pattern config (reconstructed from content).
    /// Used for diff calculation and API compatibility.
    pub fn config(&self) -> PatternConfig {
        PatternConfig {
            name: self.content.name.clone(),
            voice: self.content.voice,
            steps: self.content.steps.clone(),
            length: self.content.length,
            swing: self.content.swing,
        }
    }

    /// Queue a content swap to be applied at the specified quantization boundary.
    pub fn queue_content_swap(&mut self, new_content: Arc<PatternContent>, quant: ChangeQuant) {
        self.pending_content = Some((new_content, quant));
    }

    /// Apply pending content swap if conditions are met.
    /// Returns true if content was swapped.
    pub fn try_apply_pending(
        &mut self,
        current_beat: Beat,
        time_sig: crate::types::TimeSignature,
        tolerance: f64,
    ) -> bool {
        if let Some((new_content, quant)) = &self.pending_content {
            if quant.should_apply(
                current_beat,
                self.loop_position,
                self.content.length,
                time_sig,
                tolerance,
            ) {
                let new_content = new_content.clone();
                self.pending_content = None;

                // If length changed and we're past the new length, reset to start
                if self.loop_position >= new_content.length {
                    self.loop_position = Beat::ZERO;
                }

                self.content = new_content;
                return true;
            }
        }
        false
    }
}

/// Internal state for a melody.
///
/// Separates immutable content (notes, voice, length) from mutable playback state
/// (playing, loop_position) to enable seamless hot reload. Content can be swapped
/// atomically at musical boundaries without affecting playback continuity.
#[derive(Clone, Debug)]
pub struct MelodyState {
    /// Melody ID.
    pub id: MelodyId,

    /// The melody content (notes, voice, length, etc.).
    /// Wrapped in Arc for cheap cloning and atomic swapping during hot reload.
    pub content: Arc<MelodyContent>,

    /// Whether the melody is playing.
    pub playing: bool,

    /// Current loop position.
    pub loop_position: Beat,

    /// Pending content swap for hot reload.
    /// When set, the new content will be applied at the specified quantization boundary.
    pub pending_content: Option<(Arc<MelodyContent>, ChangeQuant)>,
}

impl MelodyState {
    /// Create a new melody state from a config.
    pub fn new(id: MelodyId, config: MelodyConfig) -> Self {
        Self {
            id,
            content: MelodyContent::arc_from_config(&config),
            playing: false,
            loop_position: Beat::ZERO,
            pending_content: None,
        }
    }

    /// Get the melody config (reconstructed from content).
    /// Used for diff calculation and API compatibility.
    pub fn config(&self) -> MelodyConfig {
        MelodyConfig {
            name: self.content.name.clone(),
            voice: self.content.voice,
            notes: self.content.notes.clone(),
            length: self.content.length,
            swing: self.content.swing,
        }
    }

    /// Queue a content swap to be applied at the specified quantization boundary.
    pub fn queue_content_swap(&mut self, new_content: Arc<MelodyContent>, quant: ChangeQuant) {
        self.pending_content = Some((new_content, quant));
    }

    /// Apply pending content swap if conditions are met.
    /// Returns true if content was swapped.
    pub fn try_apply_pending(
        &mut self,
        current_beat: Beat,
        time_sig: crate::types::TimeSignature,
        tolerance: f64,
    ) -> bool {
        if let Some((new_content, quant)) = &self.pending_content {
            if quant.should_apply(
                current_beat,
                self.loop_position,
                self.content.length,
                time_sig,
                tolerance,
            ) {
                let new_content = new_content.clone();
                self.pending_content = None;

                // If length changed and we're past the new length, reset to start
                if self.loop_position >= new_content.length {
                    self.loop_position = Beat::ZERO;
                }

                self.content = new_content;
                return true;
            }
        }
        false
    }
}

/// Internal state for a sequence.
#[derive(Clone, Debug)]
pub struct SequenceState {
    /// Sequence ID.
    pub id: SequenceId,

    /// Configuration.
    pub config: SequenceConfig,

    /// Whether the sequence is playing.
    pub playing: bool,

    /// Whether the sequence is paused.
    pub paused: bool,

    /// Whether to loop.
    pub looping: bool,

    /// Current position within the sequence.
    pub position: Beat,

    /// Global beat when this sequence started playing.
    /// Used to calculate elapsed time for proper position tracking.
    pub start_beat: Option<Beat>,
}

/// Internal state for an effect.
#[derive(Clone, Debug)]
pub struct EffectState {
    /// Effect ID.
    pub id: EffectId,

    /// Group this effect belongs to.
    pub group: GroupId,

    /// SynthDef name.
    pub synthdef: String,

    /// SuperCollider node ID.
    pub node_id: NodeId,

    /// Audio bus this effect processes (reads from and writes to).
    pub audio_bus: BusId,

    /// Current parameter values.
    pub params: ParamMap,
}

/// Internal state for a modulator.
///
/// Modulators are control-rate synths that output to control buses,
/// which can then be routed to voice parameters for modulation.
#[derive(Clone, Debug)]
pub struct ModulatorState {
    /// Modulator ID.
    pub id: ModulatorId,

    /// Configuration.
    pub config: ModulatorConfig,

    /// Control bus ID this modulator writes to.
    pub control_bus: ControlBusId,

    /// SuperCollider node ID for the modulator synth.
    pub synth_node: NodeId,
}

/// A free-list allocator that reclaims IDs when entities are deleted.
///
/// Freed IDs are pushed to a queue and reused before allocating new ones,
/// preventing unbounded ID growth over long sessions.
#[derive(Clone, Debug)]
pub struct FreeListAllocator {
    next: u32,
    free_list: VecDeque<u32>,
    min: u32,
    max: u32,
}

impl FreeListAllocator {
    pub fn new(min: u32, max: u32) -> Self {
        Self {
            next: min,
            free_list: VecDeque::new(),
            min,
            max,
        }
    }

    /// Allocate an ID, reusing a freed one if available.
    pub fn alloc(&mut self) -> Option<u32> {
        if let Some(id) = self.free_list.pop_front() {
            return Some(id);
        }
        if self.next >= self.max {
            return None;
        }
        let id = self.next;
        self.next += 1;
        Some(id)
    }

    /// Return an ID to the free list for reuse.
    pub fn free(&mut self, id: u32) {
        debug_assert!(id >= self.min && id < self.max, "freed ID out of range");
        self.free_list.push_back(id);
    }

    /// Reset to initial state (clears free list and restarts from min).
    pub fn reset(&mut self) {
        self.next = self.min;
        self.free_list.clear();
    }

    /// Total IDs ever allocated from the counter (not counting recycled ones).
    pub fn allocated_count(&self) -> u32 {
        self.next - self.min
    }
}

/// Allocator for control buses.
///
/// Control buses are used for modulation signals (LFOs, envelopes, etc.).
/// SuperCollider has 16384 control buses by default. We start allocation
/// at a safe offset to avoid conflicts with any internal usage.
#[derive(Clone, Debug)]
pub struct ControlBusAllocator {
    inner: FreeListAllocator,
}

impl Default for ControlBusAllocator {
    fn default() -> Self {
        Self::new(1000) // Start at bus 1000 to avoid conflicts
    }
}

impl ControlBusAllocator {
    /// Create a new control bus allocator starting at the given bus ID.
    pub fn new(start: u32) -> Self {
        Self {
            inner: FreeListAllocator::new(start, u32::MAX),
        }
    }

    /// Allocate a new control bus.
    pub fn allocate(&mut self) -> ControlBusId {
        ControlBusId::new(self.inner.alloc().expect("control bus IDs exhausted"))
    }

    /// Return a control bus to the pool for reuse.
    pub fn free(&mut self, bus: ControlBusId) {
        self.inner.free(bus.raw());
    }

    /// Reset the allocator to its initial state.
    ///
    /// Useful when reloading scripts and all modulators are being recreated.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Get the number of buses ever allocated from the counter.
    pub fn allocated_count(&self) -> u32 {
        self.inner.allocated_count()
    }
}

/// Allocator for audio buses.
///
/// Audio buses route audio-rate signals between groups, voices and effects.
/// Unlike control buses, an allocation may span multiple consecutive bus IDs
/// — `In.ar(bus, 2)` reads `bus` and `bus+1`, so a stereo allocation reserves
/// a pair `(id, id+1)`.
///
/// Freed chunks go to a FIFO and are reused by a later allocation requesting
/// the same channel count. The starting ID of a stereo pair is preserved on
/// reuse, so the consecutive pair stays intact.
///
/// Reuse safety: callers must only invoke [`free`](Self::free) once the
/// `Out`/`In` synth nodes that were mapped to the bus have been removed by
/// the backend. The reload diff in `handlers/groups.rs::finalize` already
/// tears down link synths before recreating groups, so reusing an audio bus
/// after that fence cannot leak audio from the prior tenant.
#[derive(Clone, Debug)]
pub struct AudioBusAllocator {
    next: u32,
    free_list: VecDeque<(u32, u8)>,
    min: u32,
    max: u32,
}

impl Default for AudioBusAllocator {
    fn default() -> Self {
        Self::new(16) // Reserve buses 0-15 for hardware I/O (bus 0 = main stereo out)
    }
}

impl AudioBusAllocator {
    /// Create a new audio bus allocator starting at the given bus ID.
    pub fn new(start: u32) -> Self {
        Self {
            next: start,
            free_list: VecDeque::new(),
            min: start,
            max: u32::MAX,
        }
    }

    /// Allocate a contiguous audio bus chunk of `channels` consecutive IDs.
    ///
    /// Reuses a freed chunk with a matching channel count if one exists,
    /// otherwise carves a fresh chunk from the monotonic counter.
    pub fn alloc(&mut self, channels: u8) -> BusId {
        debug_assert!(channels >= 1, "channels must be >= 1");
        if let Some(idx) = self.free_list.iter().position(|&(_, c)| c == channels) {
            let (id, _) = self
                .free_list
                .remove(idx)
                .expect("position returned a valid index");
            return BusId::new(id);
        }
        let id = self.next;
        let span = channels as u32;
        let new_next = self
            .next
            .checked_add(span)
            .expect("audio bus IDs exhausted");
        assert!(new_next <= self.max, "audio bus IDs exhausted");
        self.next = new_next;
        BusId::new(id)
    }

    /// Return a previously allocated chunk to the pool for reuse.
    ///
    /// `channels` must match the value passed to [`alloc`](Self::alloc) for
    /// this `id` — the chunk is only handed out again to a request of the
    /// same width.
    pub fn free(&mut self, id: BusId, channels: u8) {
        debug_assert!(channels >= 1, "channels must be >= 1");
        debug_assert!(
            id.raw() >= self.min,
            "freed audio bus ID below allocator min"
        );
        self.free_list.push_back((id.raw(), channels));
    }

    /// Reset the allocator to its initial state.
    pub fn reset(&mut self) {
        self.next = self.min;
        self.free_list.clear();
    }

    /// Total bus IDs ever drawn from the monotonic counter (excluding reuse).
    pub fn allocated_count(&self) -> u32 {
        self.next - self.min
    }

    /// Number of chunks currently in the free-list pool.
    pub fn free_pool_size(&self) -> usize {
        self.free_list.len()
    }
}

/// Internal state for a loaded SFZ instrument.
#[derive(Clone, Debug)]
pub struct SfzInstrumentState {
    /// SFZ instrument ID.
    pub id: SfzId,

    /// Path to the SFZ file.
    pub path: PathBuf,

    /// The loaded SFZ instrument data (regions, opcodes, etc.).
    /// Contains all sample mappings and playback parameters.
    pub regions: Vec<SfzRegionState>,

    /// Round-robin state for cycling through alternating samples.
    /// Key: (note, velocity_layer), Value: current position.
    pub round_robin_state: HashMap<(u8, u8), u32>,
}

/// State for a single SFZ region (sample mapping).
#[derive(Clone, Debug)]
pub struct SfzRegionState {
    /// Buffer ID for the sample.
    pub buffer_id: BufferId,

    /// Number of channels in the sample.
    pub num_channels: u8,

    /// Key range (low, high).
    pub key_range: (u8, u8),

    /// Velocity range (low, high).
    pub vel_range: (u8, u8),

    /// Pitch keycenter (the note the sample was recorded at).
    pub pitch_keycenter: u8,

    /// Sequence position for round-robin (0 = no round-robin).
    pub seq_position: u32,

    /// Sequence length for round-robin (0 = no round-robin).
    pub seq_length: u32,

    /// Amplitude envelope: attack time in seconds.
    pub ampeg_attack: f32,

    /// Amplitude envelope: decay time in seconds.
    pub ampeg_decay: f32,

    /// Amplitude envelope: sustain level (0.0-1.0).
    pub ampeg_sustain: f32,

    /// Amplitude envelope: release time in seconds.
    pub ampeg_release: f32,

    /// Volume offset in dB.
    pub volume: f32,

    /// Pan position (-1.0 to 1.0).
    pub pan: f32,

    /// Transpose in semitones.
    pub transpose: i8,

    /// Fine tune in cents.
    pub tune: f32,

    /// Loop mode.
    pub loop_enabled: bool,

    /// Loop start position in frames.
    pub loop_start: u32,

    /// Loop end position in frames.
    pub loop_end: u32,

    /// Sample start offset in frames.
    pub offset: u32,

    /// Filter cutoff frequency (if filter is enabled).
    pub cutoff: Option<f32>,

    /// Filter resonance (if filter is enabled).
    pub resonance: Option<f32>,
}

impl Default for SfzRegionState {
    fn default() -> Self {
        Self {
            buffer_id: BufferId::new(0),
            num_channels: 2,
            key_range: (0, 127),
            vel_range: (0, 127),
            pitch_keycenter: 60,
            seq_position: 0,
            seq_length: 0,
            ampeg_attack: 0.001,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.01,
            volume: 0.0,
            pan: 0.0,
            transpose: 0,
            tune: 0.0,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            offset: 0,
            cutoff: None,
            resonance: None,
        }
    }
}

/// Meter level for a group's audio output.
///
/// Updated at ~20Hz by SendTrig messages from the link synth.
#[derive(Clone, Debug)]
pub struct MeterLevel {
    /// Peak level for left channel (0.0 to 1.0+, can exceed for clipping).
    pub peak_left: f32,
    /// Peak level for right channel.
    pub peak_right: f32,
    /// RMS level for left channel.
    pub rms_left: f32,
    /// RMS level for right channel.
    pub rms_right: f32,
    /// When this meter was last updated.
    pub last_update: Option<Instant>,
}

impl Default for MeterLevel {
    fn default() -> Self {
        Self {
            peak_left: 0.0,
            peak_right: 0.0,
            rms_left: 0.0,
            rms_right: 0.0,
            last_update: None,
        }
    }
}

impl MeterLevel {
    /// Check if the meter data is stale (not updated recently).
    ///
    /// Returns true if no update in the last 200ms (~4 missed updates at 20Hz).
    pub fn is_stale(&self) -> bool {
        match self.last_update {
            Some(t) => t.elapsed() > std::time::Duration::from_millis(200),
            None => true,
        }
    }

    /// Get decayed meter values (returns zeros if stale).
    pub fn decayed(&self) -> (f32, f32, f32, f32) {
        if self.is_stale() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (
                self.peak_left,
                self.peak_right,
                self.rms_left,
                self.rms_right,
            )
        }
    }
}

/// An active fade operation.
#[derive(Clone, Debug)]
pub struct ActiveFade {
    /// Fade configuration.
    pub config: FadeConfig,

    /// When the fade started.
    pub start_time: Instant,

    /// Starting value.
    pub start_value: f32,
}

impl ActiveFade {
    /// Calculate the current interpolated value.
    pub fn current_value(&self, now: Instant, tempo: f64) -> f32 {
        let elapsed_secs = now.duration_since(self.start_time).as_secs_f64();
        let duration_secs = self.config.duration.to_beats() * 60.0 / tempo;

        if elapsed_secs >= duration_secs {
            return self.config.to;
        }

        let t = (elapsed_secs / duration_secs) as f32;

        // Apply curve transformation using FadeCurve::apply()
        let curved_t = self.config.curve.apply(t);

        self.start_value + (self.config.to - self.start_value) * curved_t
    }

    /// Check if the fade is complete.
    pub fn is_complete(&self, now: Instant, tempo: f64) -> bool {
        let elapsed_secs = now.duration_since(self.start_time).as_secs_f64();
        let duration_secs = self.config.duration.to_beats() * 60.0 / tempo;
        elapsed_secs >= duration_secs
    }
}

/// Complete runtime state.
///
/// This struct holds all state needed for audio playback. It is wrapped
/// in an `Arc<RwLock<State>>` and shared between handlers.
#[derive(Clone, Debug)]
pub struct State {
    // =========================================================================
    // Transport
    // =========================================================================
    /// Tempo in BPM.
    pub tempo: f64,

    /// Time signature.
    pub time_sig: TimeSignature,

    /// Current beat position.
    pub current_beat: Beat,

    /// Whether playback is running.
    pub playing: bool,

    // =========================================================================
    // Resources
    // =========================================================================
    /// Loaded synthdef names.
    pub synthdefs: HashSet<String>,

    /// Per-synthdef output port descriptors (port name + channel count).
    ///
    /// Populated at synthdef registration when explicit ports are declared.
    /// Synthdefs not in this map are treated as legacy and resolve to the
    /// implicit `[("out", 2)]` set by [`State::synthdef_outputs`].
    pub synthdef_outputs: HashMap<String, Vec<OutputPort>>,

    /// Loaded samples.
    pub samples: HashMap<SampleId, SampleInfo>,

    /// Script-allocated audio-rate buffers (alive across hot-reload).
    ///
    /// Mirrors [`crate::reload::ScriptState::buffers`] — each entry
    /// corresponds to a `b_alloc`-ed SC buffer with `BufferId == BufferId.0`
    /// used directly as the SC bufnum. Created on reload when the script
    /// adds an `allocate_buffer(name, ...)` call; freed on reload when the
    /// call is removed.
    pub buffers: HashMap<BufferId, BufferConfig>,

    /// Loaded SFZ instruments.
    pub sfz_instruments: HashMap<SfzId, SfzInstrumentState>,

    // =========================================================================
    // Entities
    // =========================================================================
    /// Groups.
    pub groups: HashMap<GroupId, GroupState>,

    /// Voices.
    pub voices: HashMap<VoiceId, VoiceState>,

    /// Patterns.
    pub patterns: HashMap<PatternId, PatternState>,

    /// Melodies.
    pub melodies: HashMap<MelodyId, MelodyState>,

    /// Sequences.
    pub sequences: HashMap<SequenceId, SequenceState>,

    /// Effects.
    pub effects: HashMap<EffectId, EffectState>,

    /// Modulators.
    pub modulators: HashMap<ModulatorId, ModulatorState>,

    // =========================================================================
    // Control Buses
    // =========================================================================
    /// Control bus allocator for modulation signals.
    pub control_buses: ControlBusAllocator,

    /// Node ID of the modulator group (runs before voice groups).
    ///
    /// All modulator synths are placed in this group to ensure they
    /// are processed before voices read from their control buses.
    pub modulator_group: Option<NodeId>,

    // =========================================================================
    // Active Playback
    // =========================================================================
    /// Active fade operations.
    pub active_fades: Vec<ActiveFade>,

    /// Fade configs tracked for reload diffing.
    ///
    /// Maps FadeId to the FadeConfig that was used to start the fade.
    /// This allows calculate_diff to detect unchanged fades and skip re-firing them.
    pub fade_configs: HashMap<FadeId, FadeConfig>,

    /// Active audio recordings (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub recordings: HashMap<RecordingId, RecordingInfo>,

    // =========================================================================
    // Metering
    // =========================================================================
    /// Meter levels for groups, keyed by link synth node ID.
    ///
    /// The link synth sends SendTrig messages with meter data at ~20Hz.
    /// The key is the node ID of the link synth (not the group ID).
    pub meter_levels: HashMap<NodeId, MeterLevel>,

    // =========================================================================
    // ID Allocation
    // =========================================================================
    /// Node ID allocator — reclaims IDs when nodes are freed.
    pub node_ids: FreeListAllocator,

    /// Buffer ID allocator — reclaims IDs when buffers are freed.
    pub buffer_ids: FreeListAllocator,

    /// Audio bus allocator.
    ///
    /// Starts at 16 because buses 0-15 are reserved for hardware I/O
    /// (bus 0 is the main stereo output). Frees go to a free-list and are
    /// reused, keeping bus IDs bounded across long live-reload sessions.
    pub audio_buses: AudioBusAllocator,

    // =========================================================================
    // Routing
    // =========================================================================
    /// Per-route mixer synth node IDs, keyed by `(voice_id, port_name)`.
    ///
    /// Populated by [`crate::handlers::RoutesHandler::finalize`] when a route
    /// is added; drained when a route is removed/changed or its owning voice
    /// is deleted. The synth reads the voice's port audio bus and writes to
    /// the destination group's audio bus (or bus 0 for `RouteDest::Main`).
    pub route_synths: HashMap<(VoiceId, String), NodeId>,

    /// Default per-voice port routes installed at voice-create time.
    ///
    /// Story 5: when a voice is created, the runtime walks the synthdef's
    /// declared output ports and writes a count-based default destination for
    /// each one (see [`crate::handlers::default_routes_for_voice`]). These
    /// entries are merged with the script-supplied `ScriptState::routes` at
    /// reload time, with explicit user routes taking precedence over defaults.
    /// Drained on voice delete so a re-created voice gets fresh defaults.
    pub default_routes: HashMap<(VoiceId, String), crate::handlers::RouteDest>,

    /// Active SET Param-route mappings (`.to_param` verb): source
    /// `(voice_id, port_name)` → list of `(target_voice_id, target_param_name)`
    /// currently `/n_map`-bound directly to the source's control bus.
    ///
    /// Multi-output v2 split SET vs BEND: SET overrides the user's `set_param`
    /// while the mapping is active, so multi-source on the same target is
    /// rejected at script time. [`Self::take_voice_param_routes`] drains both
    /// source-side and target-side appearances on voice delete.
    pub param_routes_set: HashMap<(VoiceId, String), Vec<(VoiceId, String)>>,

    /// Active BEND Param-route mappings (`.modulate_by` verb): source
    /// `(voice_id, port_name)` → list of `(target_voice_id, target_param_name)`
    /// whose target param is `/n_map`-bound to a `param_kr_modulate_<n>`
    /// summer's intermediate control bus.
    ///
    /// Multi-output v2 split SET vs BEND: BEND adds source signal(s) on top
    /// of the user's `set_param` baseline, so the runtime always spawns a
    /// summer (even at N=1). Multi-source fan-in is supported up to
    /// [`vibelang_dsp::system_synthdefs::PARAM_KR_MODULATE_MAX`].
    pub param_routes_bend: HashMap<(VoiceId, String), Vec<(VoiceId, String)>>,

    /// Active BEND-path summer synths, keyed by the target side
    /// `(target_voice_id, target_param_name)`.
    ///
    /// Multi-output v2 BEND: every `modulate_by` target gets a
    /// `param_kr_modulate_<n>` synth that reads the user's set_param value
    /// from `baseline` plus each source's control bus, and writes the sum to
    /// an intermediate control bus. The target's `/n_map` binds to the
    /// intermediate bus. The stored tuple is `(summer_node, intermediate_bus,
    /// arity_n)`; `arity_n` lets the runtime tell whether an arity change
    /// requires a fresh summer or just a `/n_set baseline` poke. Maintained
    /// exclusively by [`crate::handlers::RoutesHandler::finalize_params`].
    pub param_summers: HashMap<(VoiceId, String), (NodeId, BusId, u8)>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tempo: 120.0,
            time_sig: TimeSignature::default(),
            current_beat: Beat::ZERO,
            playing: false,
            synthdefs: HashSet::new(),
            synthdef_outputs: HashMap::new(),
            samples: HashMap::new(),
            buffers: HashMap::new(),
            sfz_instruments: HashMap::new(),
            groups: HashMap::new(),
            voices: HashMap::new(),
            patterns: HashMap::new(),
            melodies: HashMap::new(),
            sequences: HashMap::new(),
            effects: HashMap::new(),
            modulators: HashMap::new(),
            control_buses: ControlBusAllocator::default(),
            modulator_group: None,
            active_fades: Vec::new(),
            fade_configs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            recordings: HashMap::new(),
            meter_levels: HashMap::new(),
            node_ids: FreeListAllocator::new(1000, u32::MAX), // Reserve low IDs for system nodes
            buffer_ids: FreeListAllocator::new(0, u32::MAX),
            audio_buses: AudioBusAllocator::default(),
            route_synths: HashMap::new(),
            default_routes: HashMap::new(),
            param_routes_set: HashMap::new(),
            param_routes_bend: HashMap::new(),
            param_summers: HashMap::new(),
        }
    }
}

impl State {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new node ID.
    pub fn alloc_node_id(&mut self) -> NodeId {
        NodeId::new(self.node_ids.alloc().expect("node IDs exhausted"))
    }

    /// Return a node ID to the pool for reuse.
    pub fn free_node_id(&mut self, id: NodeId) {
        self.node_ids.free(id.raw());
    }

    /// Allocate a new buffer ID.
    pub fn alloc_buffer_id(&mut self) -> crate::types::BufferId {
        crate::types::BufferId::new(self.buffer_ids.alloc().expect("buffer IDs exhausted"))
    }

    /// Return a buffer ID to the pool for reuse.
    pub fn free_buffer_id(&mut self, id: crate::types::BufferId) {
        self.buffer_ids.free(id.raw());
    }

    /// Allocate an audio bus chunk of `channels` consecutive bus IDs.
    ///
    /// SuperCollider's `In.ar(bus, n)` reads `n` consecutive buses starting
    /// at `bus`, so a stereo allocation reserves the pair `(id, id+1)`.
    /// Reuses freed chunks with the same channel count when available.
    pub fn alloc_audio_bus(&mut self, channels: u8) -> BusId {
        self.audio_buses.alloc(channels)
    }

    /// Return a previously allocated audio bus chunk to the pool.
    ///
    /// `channels` must match the value passed to [`alloc_audio_bus`].
    /// Callers must ensure the corresponding `Out`/`In` synth nodes have
    /// been freed by the backend before calling this — see
    /// [`AudioBusAllocator`] for the reuse-safety contract.
    pub fn free_audio_bus(&mut self, id: BusId, channels: u8) {
        self.audio_buses.free(id, channels);
    }

    /// Allocate a stereo audio bus pair.
    ///
    /// Convenience wrapper for [`alloc_audio_bus(2)`](Self::alloc_audio_bus)
    /// — each group gets its own stereo pair for audio routing.
    pub fn alloc_bus_id(&mut self) -> BusId {
        self.alloc_audio_bus(2)
    }

    /// Allocate a control bus from the segregated control-bus free list.
    ///
    /// Used for kr-rate output ports — `Out.kr` writes to a control bus that
    /// `MapN` later maps onto a target voice param. Reuses freed IDs FIFO.
    pub fn alloc_control_bus(&mut self) -> ControlBusId {
        self.control_buses.allocate()
    }

    /// Return a control bus to the pool for reuse.
    pub fn free_control_bus(&mut self, id: ControlBusId) {
        self.control_buses.free(id);
    }

    /// Drain every route mixer synth owned by `voice_id`, returning the node
    /// IDs to free on the backend and recycling the IDs back into the node-id
    /// pool.
    ///
    /// Used by voice deletion to tear down everything a voice's routes own in
    /// one pass without needing a route diff. The caller is responsible for
    /// freeing the returned nodes on the backend.
    pub fn take_voice_route_nodes(&mut self, voice_id: VoiceId) -> Vec<NodeId> {
        let keys: Vec<(VoiceId, String)> = self
            .route_synths
            .keys()
            .filter(|k| k.0 == voice_id)
            .cloned()
            .collect();
        let mut nodes = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(node_id) = self.route_synths.remove(&key) {
                self.node_ids.free(node_id.raw());
                nodes.push(node_id);
            }
        }
        nodes
    }

    /// Drain every default-route entry owned by `voice_id` from
    /// [`State::default_routes`].
    ///
    /// Used by voice deletion so that the next reload's route diff sees the
    /// voice's defaults as removed rather than carrying them forward against
    /// a non-existent voice.
    pub fn take_voice_default_routes(&mut self, voice_id: VoiceId) {
        self.default_routes
            .retain(|(vid, _), _| *vid != voice_id);
    }

    /// Drain every Param-route entry that mentions `voice_id` from BOTH
    /// [`State::param_routes_set`] and [`State::param_routes_bend`], on
    /// either the source side or the target side.
    ///
    /// Returns the source-side drains as
    /// `((source_voice, source_port), [(target_voice, target_param), ...])`
    /// so the caller can issue `/n_map ... -1` on each `(target, param)` pair
    /// (the source's bus is going away with the voice). Target-side scrubbing
    /// is done in-place: any `(voice_id, *)` tuple in any other source's Vec
    /// is removed; entries whose Vec drops to empty after scrubbing are
    /// pruned. The deleted target voice's synth nodes are about to be freed
    /// anyway, so unmapping them is moot — only the source-side state needs
    /// caller follow-up. Set + bend drains are concatenated in the result.
    pub fn take_voice_param_routes(
        &mut self,
        voice_id: VoiceId,
    ) -> Vec<((VoiceId, String), Vec<(VoiceId, String)>)> {
        let mut drained = Vec::new();
        for map in [&mut self.param_routes_set, &mut self.param_routes_bend] {
            let source_keys: Vec<(VoiceId, String)> = map
                .keys()
                .filter(|(vid, _)| *vid == voice_id)
                .cloned()
                .collect();
            for key in source_keys {
                if let Some(targets) = map.remove(&key) {
                    drained.push((key, targets));
                }
            }
            map.retain(|_, targets| {
                targets.retain(|(t_vid, _)| *t_vid != voice_id);
                !targets.is_empty()
            });
        }
        drained
    }

    /// Resolve the output-port set for a synthdef name.
    ///
    /// Returns the explicitly declared ports if registered, otherwise the
    /// implicit legacy single stereo `out` port. Voice creation uses this to
    /// decide how many audio buses to allocate per voice.
    pub fn synthdef_outputs(&self, name: &str) -> Vec<OutputPort> {
        self.synthdef_outputs
            .get(name)
            .cloned()
            .unwrap_or_else(legacy_output_ports)
    }

    /// Convert beats to seconds at current tempo.
    pub fn beats_to_secs(&self, beats: f64) -> f64 {
        beats * 60.0 / self.tempo
    }

    /// Convert seconds to beats at current tempo.
    pub fn secs_to_beats(&self, secs: f64) -> f64 {
        secs * self.tempo / 60.0
    }
}

/// The implicit port set used for synthdefs that did not call `.output(...)`.
///
/// One stereo `out` port — exactly what voices got before multi-output landed.
pub fn legacy_output_ports() -> Vec<OutputPort> {
    vec![OutputPort {
        name: "out".to_string(),
        channels: 2,
        rate: vibelang_dsp::PortRate::Ar,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_default() {
        let state = State::default();
        assert_eq!(state.tempo, 120.0);
        assert!(!state.playing);
        assert_eq!(state.current_beat, Beat::ZERO);
    }

    #[test]
    fn test_alloc_node_id() {
        let mut state = State::new();
        let id1 = state.alloc_node_id();
        let id2 = state.alloc_node_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_beats_to_secs() {
        let state = State::new(); // 120 BPM
        assert!((state.beats_to_secs(2.0) - 1.0).abs() < 0.001);
    }

    // =========================================================================
    // FreeListAllocator tests
    // =========================================================================

    #[test]
    fn test_free_list_alloc_sequential() {
        let mut alloc = FreeListAllocator::new(10, 20);
        assert_eq!(alloc.alloc(), Some(10));
        assert_eq!(alloc.alloc(), Some(11));
        assert_eq!(alloc.alloc(), Some(12));
    }

    #[test]
    fn test_free_list_reuse_after_free() {
        let mut alloc = FreeListAllocator::new(0, 100);
        let ids: Vec<u32> = (0..10).map(|_| alloc.alloc().unwrap()).collect();
        // Free half of them
        for &id in &ids[0..5] {
            alloc.free(id);
        }
        // Next allocs should come from the free list (FIFO)
        let reused: Vec<u32> = (0..5).map(|_| alloc.alloc().unwrap()).collect();
        assert_eq!(reused, ids[0..5]);
        // After free list exhausted, continues from counter
        assert_eq!(alloc.alloc(), Some(10));
    }

    #[test]
    fn test_free_list_max_enforcement() {
        let mut alloc = FreeListAllocator::new(0, 3);
        assert_eq!(alloc.alloc(), Some(0));
        assert_eq!(alloc.alloc(), Some(1));
        assert_eq!(alloc.alloc(), Some(2));
        // Exhausted — no free list entries
        assert_eq!(alloc.alloc(), None);
    }

    #[test]
    fn test_free_list_max_with_reclaim() {
        let mut alloc = FreeListAllocator::new(0, 2);
        let a = alloc.alloc().unwrap(); // 0
        let b = alloc.alloc().unwrap(); // 1
        assert_eq!(alloc.alloc(), None); // exhausted
        alloc.free(a);
        // Now we can alloc again from the free list
        assert_eq!(alloc.alloc(), Some(0));
        alloc.free(b);
        assert_eq!(alloc.alloc(), Some(1));
    }

    #[test]
    fn test_free_list_reset() {
        let mut alloc = FreeListAllocator::new(5, 100);
        alloc.alloc();
        alloc.alloc();
        alloc.free(5);
        alloc.reset();
        // After reset, starts fresh from min
        assert_eq!(alloc.alloc(), Some(5));
    }

    #[test]
    fn test_state_free_node_id_reuse() {
        let mut state = State::new();
        let id1 = state.alloc_node_id();
        let id2 = state.alloc_node_id();
        state.free_node_id(id1);
        // Next alloc should reuse id1
        let id3 = state.alloc_node_id();
        assert_eq!(id3, id1);
        assert_ne!(id3, id2);
    }

    #[test]
    fn test_state_free_buffer_id_reuse() {
        let mut state = State::new();
        let b1 = state.alloc_buffer_id();
        let b2 = state.alloc_buffer_id();
        state.free_buffer_id(b1);
        let b3 = state.alloc_buffer_id();
        assert_eq!(b3, b1);
        assert_ne!(b3, b2);
    }

    #[test]
    fn test_control_bus_alloc_free_reuse() {
        let mut alloc = ControlBusAllocator::new(1000);
        let b1 = alloc.allocate();
        let b2 = alloc.allocate();
        assert_eq!(b1.raw(), 1000);
        assert_eq!(b2.raw(), 1001);
        alloc.free(b1);
        let b3 = alloc.allocate();
        assert_eq!(b3.raw(), 1000);
    }

    #[test]
    fn test_control_bus_reset() {
        let mut alloc = ControlBusAllocator::new(1000);
        alloc.allocate();
        alloc.allocate();
        alloc.reset();
        assert_eq!(alloc.allocate().raw(), 1000);
    }

    // =========================================================================
    // AudioBusAllocator tests
    // =========================================================================

    #[test]
    fn test_audio_bus_alloc_free_alloc_reuses() {
        let mut alloc = AudioBusAllocator::new(16);
        let a = alloc.alloc(2);
        let b = alloc.alloc(2);
        assert_eq!(a.raw(), 16);
        assert_eq!(b.raw(), 18);
        alloc.free(a, 2);
        // Second alloc of matching width reuses the freed pair.
        let c = alloc.alloc(2);
        assert_eq!(c.raw(), 16);
        // Counter did not advance for the reused chunk.
        assert_eq!(alloc.allocated_count(), 4);
    }

    #[test]
    fn test_audio_bus_stereo_pair_consecutive_on_reuse() {
        let mut alloc = AudioBusAllocator::new(16);
        let pair = alloc.alloc(2);
        // The pair occupies (pair, pair+1) — confirm the next alloc is offset by 2.
        let next = alloc.alloc(2);
        assert_eq!(next.raw(), pair.raw() + 2);

        alloc.free(pair, 2);
        let reused = alloc.alloc(2);
        assert_eq!(reused.raw(), pair.raw());
        // After reuse, a fresh stereo alloc continues from the monotonic frontier
        // without colliding with the reused pair.
        let fresh = alloc.alloc(2);
        assert_eq!(fresh.raw(), next.raw() + 2);
        assert_ne!(fresh.raw(), reused.raw());
        assert_ne!(fresh.raw(), reused.raw() + 1);
    }

    #[test]
    fn test_audio_bus_hammer_bounded_growth() {
        let mut alloc = AudioBusAllocator::new(16);
        // Prime a steady-state working set of 4 stereo buses.
        let initial: Vec<BusId> = (0..4).map(|_| alloc.alloc(2)).collect();
        let baseline_count = alloc.allocated_count();
        let mut held = initial.clone();
        for _ in 0..1000 {
            // Release the oldest, allocate a replacement — working set stays at 4.
            let evicted = held.remove(0);
            alloc.free(evicted, 2);
            held.push(alloc.alloc(2));
        }
        // The monotonic counter never advanced past the priming round.
        assert_eq!(alloc.allocated_count(), baseline_count);
        // Free pool size stays bounded — drains to zero between cycles.
        assert!(
            alloc.free_pool_size() <= 1,
            "free pool grew unboundedly: {}",
            alloc.free_pool_size()
        );
    }

    #[test]
    fn test_audio_bus_channels_segregated() {
        // A freed mono chunk is not handed out to a stereo request,
        // and vice versa — preventing accidental width mismatches.
        let mut alloc = AudioBusAllocator::new(16);
        let mono = alloc.alloc(1);
        assert_eq!(mono.raw(), 16);
        alloc.free(mono, 1);
        // Stereo alloc should NOT reuse the mono slot (different width).
        let stereo = alloc.alloc(2);
        assert_ne!(stereo.raw(), mono.raw());
        assert_eq!(stereo.raw(), 17);
        // The mono chunk is still in the pool for a matching mono request.
        let mono2 = alloc.alloc(1);
        assert_eq!(mono2.raw(), mono.raw());
    }

    #[test]
    fn test_audio_bus_reset() {
        let mut alloc = AudioBusAllocator::new(16);
        alloc.alloc(2);
        alloc.alloc(2);
        alloc.reset();
        assert_eq!(alloc.alloc(2).raw(), 16);
        assert_eq!(alloc.allocated_count(), 2);
    }

    #[test]
    fn test_state_alloc_audio_bus_reuses() {
        let mut state = State::new();
        let a = state.alloc_audio_bus(2);
        let b = state.alloc_audio_bus(2);
        assert_eq!(a.raw(), 16);
        assert_eq!(b.raw(), 18);
        state.free_audio_bus(a, 2);
        let c = state.alloc_audio_bus(2);
        assert_eq!(c.raw(), a.raw());
    }

    #[test]
    fn test_state_alloc_bus_id_back_compat() {
        // The legacy stereo helper still produces consecutive pairs.
        let mut state = State::new();
        let a = state.alloc_bus_id();
        let b = state.alloc_bus_id();
        assert_eq!(b.raw(), a.raw() + 2);
    }
}
