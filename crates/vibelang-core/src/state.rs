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
    BufferConfig, FadeConfig, MelodyConfig, MelodyContent, PatternConfig, PatternContent,
    SampleInfo, SequenceConfig, VoiceConfig,
};
use crate::types::FadeId;
#[cfg(not(target_arch = "wasm32"))]
use crate::types::RecordingId;
use crate::types::{
    Beat, BufferId, BusId, ControlBusId, EffectId, GroupId, MelodyId, NodeId, ParamMap, PatternId,
    SampleId, SequenceId, SfzId, TimeSignature, VoiceId,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use vibelang_dsp::{InputPort, OutputPort};

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

    /// Number of hardware output channels this group writes to.
    /// `Some(1)` selects the `system_link_audio_mono` mixdown variant;
    /// `Some(2)` selects the stereo `system_link_audio` (default behaviour).
    /// Coupled with `output_bus`: must be `Some(_)` iff `output_bus` is
    /// `Some(_)`. Mirrored from `GroupConfig::output_channels`.
    pub output_channels: Option<u32>,
}

/// Runtime routing role for a voice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VoiceRole {
    /// Voice may receive implicit audible defaults.
    #[default]
    Audible,
    /// Voice is intended only as a modulation source; implicit audible defaults are suppressed.
    ModulatorOnly,
}

/// Internal state for a voice.
#[derive(Clone, Debug)]
pub struct VoiceState {
    /// Voice ID.
    pub id: VoiceId,

    /// Configuration.
    pub config: VoiceConfig,

    /// Effective routing role derived during reload diffing.
    pub role: VoiceRole,

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

    /// Audio buses owned by this voice, one per declared **input** port.
    ///
    /// TODO(P1.3): allocation of these buses at voice creation is not yet
    /// implemented. The field exists so the named-inputs dispatcher
    /// ([`crate::handlers::RoutesHandler::finalize_input_routes`]) can resolve
    /// each `(target_voice, input_port_name)` edge to a concrete bus; until
    /// P1.3 lands, the vec stays empty on production paths and the dispatcher
    /// debug-logs + no-ops for any route whose target has no matching entry.
    /// Tests populate this directly to exercise the dispatcher.
    pub input_buses: Vec<(String, BusId)>,
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

    /// Absolute beat at which this pattern was last `start()`-ed.
    ///
    /// Playback positions in the tick loop are computed as
    /// `(current_beat - start_beat) % length`, so the pattern always plays
    /// from its own beat 0 instead of phase-locking to absolute song time.
    /// Without this, a looper that finalises a recording at absolute beat
    /// 17 with a 4-beat length would begin playback at offset
    /// `17 % 4 = 1.0` — the middle of the recording.
    pub start_beat: Beat,

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
            start_beat: Beat::ZERO,
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

    /// Per-synthdef input port descriptors (port name + channel count + rate).
    ///
    /// Populated at synthdef registration alongside [`Self::synthdef_outputs`].
    /// Synthdefs not in this map have no declared inputs.
    pub synthdef_inputs: HashMap<String, Vec<InputPort>>,

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

    // =========================================================================
    // Control Buses
    // =========================================================================
    /// Control bus allocator for modulation signals.
    pub control_buses: ControlBusAllocator,

    /// Node ID of the param-summer group (runs at the head of root so
    /// `param_kr_modulate_<n>` and `param_kr_sum_<n>` summers tick before
    /// voice groups read their `/n_map`-bound params).
    ///
    /// Lazily created by [`crate::handlers::RoutesHandler::finalize_params`]
    /// the first time it needs to spawn a summer.
    pub param_summer_group: Option<NodeId>,

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
    /// Per-edge mixer synth node IDs, keyed by `(voice_id, port_name, dest)`.
    ///
    /// One entry per `(voice, port) → dest` edge — a port that fans out to
    /// multiple group destinations has one entry per destination. Populated
    /// by [`crate::handlers::RoutesHandler::finalize`] when an edge is added;
    /// drained when an edge is removed or its owning voice is deleted. The
    /// synth reads the voice's port audio bus and writes to the destination
    /// group's audio bus (or bus 0 for `RouteDest::Main`).
    pub route_synths: HashMap<(VoiceId, String, crate::handlers::RouteDest), NodeId>,

    /// Last applied per-voice routing map.
    ///
    /// Updated at the end of `Runtime::apply_reload` so the next reload can
    /// produce a route diff against this baseline. Kept beside
    /// [`Self::route_synths`] so the materialized route nodes and their
    /// desired-route snapshot stay under the same state lock.
    pub current_routes: crate::handlers::RouteMap,

    /// Per-voice **input** routes, keyed by `(target_voice_id, input_port_name)`.
    ///
    /// Symmetric to the output-side [`crate::handlers::RouteMap`]: the value is
    /// the list of sources feeding a single named input port. Per the
    /// named-inputs design (kb/named-inputs-design-notes.md decision #1),
    /// `voice.input("name").from(x)` produces a Vec of length 1 (replace) and
    /// `voice.input("name").from_all([…])` / `.add_from(x)` produces a Vec of
    /// length > 1 (explicit fan-in). Diffed by
    /// [`crate::handlers::compute_input_route_diff`]; the dispatcher and bus
    /// resolution land in P3.3.
    pub input_routes: crate::handlers::InputRouteMap,

    /// Per-edge **input** router synth node IDs, keyed by
    /// `(target_voice_id, input_port_name, source)`.
    ///
    /// Symmetric to [`Self::route_synths`] on the output side. Populated by
    /// [`crate::handlers::RoutesHandler::finalize_input_routes`] when an input
    /// edge is added; drained when an edge is removed or its owning voice is
    /// deleted. The router synth (`input_link_1` / `input_link_2`) reads the
    /// source bus and writes to the target voice's named-input bus; fan-in
    /// sums naturally on the destination bus when multiple routers share it.
    pub input_route_synths: HashMap<(VoiceId, String, crate::handlers::InputRouteSrc), NodeId>,

    /// Shared silent audio (ar) bus.
    ///
    /// Per kb/named-inputs-design-notes.md decision #2, every declared ar input
    /// port has a valid bus even when nothing is routed in: an `input_link_*`
    /// router reads from this bus, which scsynth keeps zero-filled because
    /// nothing writes to it. Allocated lazily by
    /// [`crate::handlers::RoutesHandler::finalize_input_routes`] on first
    /// `Silent` ar route and never freed.
    pub silent_ar_bus: Option<BusId>,

    /// Shared silent control (kr) bus.
    ///
    /// Per kb/named-inputs-design-notes.md decision #2, every declared kr input
    /// port has a valid bus even when nothing is routed in. Allocated lazily
    /// from [`Self::control_buses`] by
    /// [`crate::handlers::RoutesHandler::finalize_input_routes`] on first
    /// `Silent` kr route and never freed.
    pub silent_kr_bus: Option<ControlBusId>,

    /// Starting bus index for hardware *inputs* in scsynth's contiguous audio
    /// bus layout (`[outputs | inputs | private]`).
    ///
    /// Equals `output_channels` (the count passed to
    /// [`Self::with_audio_config`]) so a route source
    /// `InputRouteSrc::HardwareInput(vec![ch])` resolves to bus
    /// `hardware_input_offset + ch`. Defaults to 0 — combined with the default
    /// `AudioBusAllocator`'s `min = 16`, this leaves the legacy
    /// `[0..16)` hardware reserve in place but does not promise any specific
    /// input-channel layout. Production paths set this via
    /// [`Self::with_audio_config`].
    pub hardware_input_offset: u32,

    /// Default per-voice port routes installed at voice-create time.
    ///
    /// Story 5: when a voice is created, the runtime walks the synthdef's
    /// declared output ports and writes a count-based default destination for
    /// each one (see [`crate::handlers::default_routes_for_voice`]). These
    /// entries are merged with the script-supplied `ScriptState::routes` at
    /// reload time, with explicit user routes taking precedence over defaults.
    /// Drained on voice delete so a re-created voice gets fresh defaults.
    ///
    /// Defaults are stored as `Vec<RouteDest>` to match [`RouteMap`]'s shape;
    /// a port's default is conventionally a single-element vec.
    pub default_routes: HashMap<(VoiceId, String), Vec<crate::handlers::RouteDest>>,

    /// Active SET Param-route mappings (`.to_param` verb): source
    /// `(voice_id, port_name)` → list of `(target, target_param_name)`
    /// currently `/n_map`-bound directly to the source's control bus. The
    /// target side is a [`crate::handlers::ParamRouteTarget`] enum so a
    /// single source can fan out to any mix of voice and fx targets.
    ///
    /// Multi-output v2 split SET vs BEND: SET overrides the user's `set_param`
    /// while the mapping is active, so multi-source on the same target is
    /// rejected at script time. [`Self::take_voice_param_routes`] drains both
    /// source-side and target-side voice appearances on voice delete;
    /// [`Self::take_effect_param_routes`] drains target-side fx appearances on
    /// effect delete.
    pub param_routes_set: crate::handlers::ParamRouteMap,

    /// Active BEND Param-route mappings (`.modulate_by` verb): source
    /// `(voice_id, port_name)` → list of `(target, target_param_name)`
    /// whose target param is `/n_map`-bound to a `param_kr_modulate_<n>`
    /// summer's intermediate control bus.
    ///
    /// Multi-output v2 split SET vs BEND: BEND adds source signal(s) on top
    /// of the user's `set_param` baseline, so the runtime always spawns a
    /// summer (even at N=1). Multi-source fan-in is supported up to
    /// [`vibelang_dsp::system_synthdefs::PARAM_KR_MODULATE_MAX`].
    pub param_routes_bend: crate::handlers::ParamRouteMap,

    /// Active TRIGGER Param-route mappings (`.to_trigger` verb): source
    /// `(voice_id, port_name)` → list of `(target, target_param_name)`
    /// whose target param is `/n_map`-bound to a `port_tr_to_param_link_1`
    /// link synth's intermediate control bus.
    ///
    /// Multi-output v3 B2.c trigger path: Tr-rate sources forward
    /// sample-accurate edges to a target param without scale/offset shaping
    /// (triggers don't bend). Single-source per `(target, param)`; multi-
    /// source fan-in is rejected at script time. Cross-verb-exclusive with
    /// [`Self::param_routes_set`] and [`Self::param_routes_bend`].
    pub param_routes_trigger: crate::handlers::ParamRouteMap,

    /// Applied per-source scale/offset shaping for active SET routes.
    ///
    /// Keyed by `(source_voice, source_port, target, target_param)`, matching
    /// `ScriptState::param_route_set_shaping`. Kept in runtime state so
    /// shaping-only reloads can diff and respawn the relevant summer.
    pub param_route_set_shaping:
        HashMap<(VoiceId, String, crate::handlers::ParamRouteTarget, String), (f32, f32)>,

    /// Applied per-source scale/offset shaping for active BEND routes.
    pub param_route_bend_shaping:
        HashMap<(VoiceId, String, crate::handlers::ParamRouteTarget, String), (f32, f32)>,

    /// Active param-summer synths, keyed by the target side
    /// `(target, target_param_name)`.
    ///
    /// Multi-output v3 unified routing: every routed param target gets a
    /// `param_kr_modulate_<n>` synth that computes
    /// `baseline + Σ (scale_i * In.kr(in_i, 1) + offset_i)` and writes the
    /// result to an intermediate control bus. The target's `/n_map` binds to
    /// that bus. SET routes pin `baseline=0` and use the per-source
    /// scale/offset defaults (1.0 / 0.0). BEND routes drive `baseline` from
    /// the user's `set_param` value; per-source scale/offset still apply.
    /// `sources` records each contributing source's `(bus, scale, offset)`
    /// in the same letter order (`a`..`h`) as the synthdef's params, so the
    /// runtime can update `scale_<i>` / `offset_<i>` without rebuilding the
    /// summer. Maintained exclusively by
    /// [`crate::handlers::RoutesHandler::finalize_params`].
    /// `param_summers` and `route_synths` are always mutated together under
    /// the state write lock; reading one without the other is a bug.
    pub param_summers: HashMap<(crate::handlers::ParamRouteTarget, String), ParamSummerState>,

    /// Active `a2k_adapter_1` synths spawned to coerce an audio-rate source
    /// port into a kr-rate signal so it can drive a `param_kr_modulate_<n>`
    /// summer just like a native kr port. Keyed by source `(voice_id,
    /// port_name)`; one adapter is shared by all `.to_param_audio()` routes
    /// that originate from the same `(source, port)` pair, since they all
    /// downsample the same audio bus.
    ///
    /// `(NodeId, ControlBusId)` records the adapter synth and the
    /// intermediate kr bus it writes to. The bus is what the summer reads
    /// (via `in_<i>`); the source's own audio bus stays untouched.
    /// Maintained exclusively by
    /// [`crate::handlers::RoutesHandler::finalize_params`].
    pub ar_to_kr_adapters: HashMap<(VoiceId, String), (NodeId, ControlBusId)>,

    /// Active `port_tr_to_param_link_1` link synths spawned by
    /// `.to_trigger` routes. Keyed by the target side
    /// `(target, target_param_name)` — the target side is a
    /// [`crate::handlers::ParamRouteTarget`] enum so fx-target trigger
    /// routes share the same registry.
    ///
    /// `(NodeId, BusId)` records the link synth node and the intermediate
    /// control bus it writes to (the target's `/n_map` destination). One
    /// link synth per `(target, param)` — single-source trigger only, no
    /// summer or scale/offset shaping. Maintained exclusively by
    /// [`crate::handlers::RoutesHandler::finalize_params`].
    pub param_triggers: HashMap<(crate::handlers::ParamRouteTarget, String), (NodeId, BusId)>,

    /// Per-voice voice-allocation pool for MIDI-output voices.
    ///
    /// Every MIDI-output voice (any `polyphony >= 1`) gets a [`MidiVoicePool`]
    /// here on first note. It collapses a polyphonic input stream onto `n`
    /// sounding slots with a clean retrigger on note steal, a return-to-held
    /// on note release (notes whose slot was stolen sit in the pool's overflow
    /// stack until their key is released), and a `NoteOff` for every sounding
    /// slot when the voice is stopped / deleted / reloaded. `poly(1)` is just
    /// the `n == 1` case. Drives [`crate::handlers::VoicesHandler::note_on`] /
    /// `note_off`. Cleared on voice stop / delete / create (reload); resized
    /// in place when `polyphony` changes on reload.
    pub midi_voice_pool: HashMap<VoiceId, MidiVoicePool>,

    /// Current note generation per `(voice, MIDI note)`.
    ///
    /// Pattern playback captures this generation when it starts a note and
    /// delayed note-offs only release if the same generation is still active.
    pub voice_note_generations: HashMap<(VoiceId, u8), u64>,

    /// Monotonic source for [`Self::voice_note_generations`].
    pub next_voice_note_generation: u64,
}

/// Per-voice voice-allocation pool for a MIDI-output voice — see
/// [`State::midi_voice_pool`].
///
/// Invariants: `slots.len() == polyphony` (`>= 1`); `alloc_order` holds
/// exactly the indices of the occupied (`Some`) slots, oldest-allocated
/// first; `overflow` holds held-but-not-sounding `(note, velocity)` pairs
/// (their slot was stolen), most-recently-stolen last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiVoicePool {
    /// Sounding `(note, velocity)` per voice slot; `None` is a free slot.
    pub slots: Vec<Option<(u8, u8)>>,
    /// Held keys whose slot was stolen, most-recently-stolen last.
    pub overflow: Vec<(u8, u8)>,
    /// Occupied slot indices, oldest-allocated first (note-steal order).
    pub alloc_order: Vec<usize>,
}

/// Live state of one `param_kr_modulate_<n>` summer instance.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamSummerState {
    /// Node ID of the summer synth.
    pub node: NodeId,
    /// Intermediate control bus the summer writes to (target's `/n_map`
    /// destination).
    pub bus: BusId,
    /// Per-source contributions in summer-arg order (`in_a`/`scale_a`/
    /// `offset_a`, `in_b`/`scale_b`/`offset_b`, …). `sources.len()` is the
    /// summer's arity `n`.
    pub sources: Vec<ParamSummerSource>,
}

impl ParamSummerState {
    /// Convenience: arity (number of source contributions).
    pub fn arity(&self) -> usize {
        self.sources.len()
    }
}

/// One source contribution inside a [`ParamSummerState`]: a control bus
/// plus the per-source affine shaping parameters that go to the
/// `param_kr_modulate_<n>` synth (`scale_<i> * In.kr(in_<i>, 1) + offset_<i>`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParamSummerSource {
    /// Source kr bus the summer reads from (`in_<i>` parameter).
    pub bus: BusId,
    /// Linear gain on this source (`scale_<i>`, default 1.0).
    pub scale: f32,
    /// DC offset added after scaling (`offset_<i>`, default 0.0).
    pub offset: f32,
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
            synthdef_inputs: HashMap::new(),
            samples: HashMap::new(),
            buffers: HashMap::new(),
            sfz_instruments: HashMap::new(),
            groups: HashMap::new(),
            voices: HashMap::new(),
            patterns: HashMap::new(),
            melodies: HashMap::new(),
            sequences: HashMap::new(),
            effects: HashMap::new(),
            control_buses: ControlBusAllocator::default(),
            param_summer_group: None,
            active_fades: Vec::new(),
            fade_configs: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            recordings: HashMap::new(),
            meter_levels: HashMap::new(),
            node_ids: FreeListAllocator::new(1000, u32::MAX), // Reserve low IDs for system nodes
            buffer_ids: FreeListAllocator::new(0, u32::MAX),
            audio_buses: AudioBusAllocator::default(),
            route_synths: HashMap::new(),
            current_routes: HashMap::new(),
            input_routes: HashMap::new(),
            input_route_synths: HashMap::new(),
            silent_ar_bus: None,
            silent_kr_bus: None,
            hardware_input_offset: 0,
            default_routes: HashMap::new(),
            param_routes_set: HashMap::new(),
            param_routes_bend: HashMap::new(),
            param_routes_trigger: HashMap::new(),
            param_route_set_shaping: HashMap::new(),
            param_route_bend_shaping: HashMap::new(),
            param_summers: HashMap::new(),
            ar_to_kr_adapters: HashMap::new(),
            param_triggers: HashMap::new(),
            midi_voice_pool: HashMap::new(),
            voice_note_generations: HashMap::new(),
            next_voice_note_generation: 1,
        }
    }
}

impl State {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new empty state with the audio bus allocator configured for
    /// the given hardware I/O.
    ///
    /// scsynth lays out audio buses contiguously: `[0..output_channels)`
    /// holds the hardware outputs, `[output_channels..output_channels +
    /// input_channels)` holds the hardware inputs, and user (private) buses
    /// begin at `output_channels + input_channels`. With `-i 8 -o 10` the
    /// boundary is bus 18, so the first user bus must be 18 — anything
    /// lower aliases onto a hardware input bus and races with live audio.
    /// The default `AudioBusAllocator::default()` hard-codes 16 (stereo out
    /// + a small reserve) and silently corrupts any setup with more than
    /// 16 hardware buses; this constructor is the hardware-aware variant.
    pub fn with_audio_config(output_channels: u32, input_channels: u32) -> Self {
        let start = output_channels.saturating_add(input_channels);
        let mut state = Self::default();
        state.audio_buses = AudioBusAllocator::new(start);
        state.hardware_input_offset = output_channels;
        state
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
        let keys: Vec<(VoiceId, String, crate::handlers::RouteDest)> = self
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

    /// Drain every named-input router synth that mentions `voice_id`, either
    /// as the target voice or as a `Voice(...)` source.
    ///
    /// Used by voice deletion/recreation so no `input_link_*` synth keeps
    /// reading from or writing to buses owned by a removed voice.
    pub fn take_voice_input_route_nodes(&mut self, voice_id: VoiceId) -> Vec<NodeId> {
        use crate::handlers::InputRouteSrc;

        let keys: Vec<(VoiceId, String, InputRouteSrc)> = self
            .input_route_synths
            .keys()
            .filter(|(target, _, src)| {
                *target == voice_id
                    || matches!(src, InputRouteSrc::Voice(src_voice, _) if *src_voice == voice_id)
            })
            .cloned()
            .collect();
        let mut nodes = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(node_id) = self.input_route_synths.remove(&key) {
                self.node_ids.free(node_id.raw());
                nodes.push(node_id);
            }
        }

        self.input_routes.retain(|(target, _), srcs| {
            if *target == voice_id {
                return false;
            }
            srcs.retain(
                |src| !matches!(src, InputRouteSrc::Voice(src_voice, _) if *src_voice == voice_id),
            );
            !srcs.is_empty()
        });

        nodes
    }

    /// Drain every default-route entry owned by `voice_id` from
    /// [`State::default_routes`].
    ///
    /// Used by voice deletion so that the next reload's route diff sees the
    /// voice's defaults as removed rather than carrying them forward against
    /// a non-existent voice.
    pub fn take_voice_default_routes(&mut self, voice_id: VoiceId) {
        self.default_routes.retain(|(vid, _), _| *vid != voice_id);
    }

    /// Return active param-route bindings that a newly-created node for
    /// `voice_id` must inherit.
    ///
    /// Param summers and trigger links are voice-target scoped, not node
    /// scoped. Route finalization maps every node that exists at that moment;
    /// note/trigger paths use this helper to map nodes spawned later onto the
    /// already-materialized intermediate buses.
    pub fn active_param_bindings_for_voice(&self, voice_id: VoiceId) -> Vec<(String, BusId)> {
        use crate::handlers::ParamRouteTarget;

        let mut bindings = Vec::new();
        for ((target, param), summer) in &self.param_summers {
            if *target == ParamRouteTarget::Voice(voice_id) {
                bindings.push((param.clone(), summer.bus));
            }
        }
        for ((target, param), (_, bus)) in &self.param_triggers {
            if *target == ParamRouteTarget::Voice(voice_id) {
                bindings.push((param.clone(), *bus));
            }
        }
        bindings.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.raw().cmp(&b.1.raw())));
        bindings
    }

    /// Drain every Param-route entry that mentions `voice_id` from
    /// [`State::param_routes_set`], [`State::param_routes_bend`], and
    /// [`State::param_routes_trigger`], on either the source side or the
    /// target side.
    ///
    /// Returns the source-side drains as
    /// `((source_voice, source_port), [(target, target_param), ...])` so the
    /// caller can issue `/n_map ... -1` on each `(target, param)` pair (the
    /// source's bus is going away with the voice). Target-side scrubbing is
    /// done in-place: any `Voice(voice_id)` tuple in any other source's Vec
    /// is removed; entries whose Vec drops to empty after scrubbing are
    /// pruned. The deleted target voice's synth nodes are about to be freed
    /// anyway, so unmapping them is moot — only the source-side state needs
    /// caller follow-up. Set + bend + trigger drains are concatenated in the
    /// result.
    pub fn take_voice_param_routes(
        &mut self,
        voice_id: VoiceId,
    ) -> Vec<(
        (VoiceId, String),
        Vec<(crate::handlers::ParamRouteTarget, String)>,
    )> {
        use crate::handlers::ParamRouteTarget;
        let mut drained = Vec::new();
        for map in [
            &mut self.param_routes_set,
            &mut self.param_routes_bend,
            &mut self.param_routes_trigger,
        ] {
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
                targets.retain(|(t, _)| *t != ParamRouteTarget::Voice(voice_id));
                !targets.is_empty()
            });
        }
        self.param_route_set_shaping
            .retain(|(source_voice, _, target, _), _| {
                *source_voice != voice_id && *target != ParamRouteTarget::Voice(voice_id)
            });
        self.param_route_bend_shaping
            .retain(|(source_voice, _, target, _), _| {
                *source_voice != voice_id && *target != ParamRouteTarget::Voice(voice_id)
            });
        drained
    }

    /// Drain every Param-route entry that mentions `effect_id` (target-side
    /// only — fx are never sources) from [`State::param_routes_set`],
    /// [`State::param_routes_bend`], and [`State::param_routes_trigger`].
    ///
    /// Effects don't own kr/tr/ar output ports, so there's nothing to drain
    /// on the source side; the only state to clean up is the appearances of
    /// `Effect(effect_id)` in target lists. Entries whose Vec drops to empty
    /// after scrubbing are pruned. Mirrors the target-side scrubbing in
    /// [`Self::take_voice_param_routes`].
    pub fn take_effect_param_routes(&mut self, effect_id: crate::types::EffectId) {
        use crate::handlers::ParamRouteTarget;
        for map in [
            &mut self.param_routes_set,
            &mut self.param_routes_bend,
            &mut self.param_routes_trigger,
        ] {
            map.retain(|_, targets| {
                targets.retain(|(t, _)| *t != ParamRouteTarget::Effect(effect_id));
                !targets.is_empty()
            });
        }
        self.param_route_set_shaping
            .retain(|(_, _, target, _), _| *target != ParamRouteTarget::Effect(effect_id));
        self.param_route_bend_shaping
            .retain(|(_, _, target, _), _| *target != ParamRouteTarget::Effect(effect_id));
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

    /// Resolve the input-port set for a synthdef name.
    ///
    /// Returns the explicitly declared ports if registered, otherwise an empty
    /// vec — synthdefs that never called `.input(...)` have no inputs to
    /// allocate buses for.
    pub fn synthdef_inputs(&self, name: &str) -> Vec<InputPort> {
        self.synthdef_inputs.get(name).cloned().unwrap_or_default()
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

    #[test]
    fn synthdef_inputs_round_trip_through_state() {
        // Build a SynthDef declaring 2 ar inputs (1 + 2 channels) and 1 kr
        // input; register the resulting `inputs` list into `State` and
        // confirm the accessor recovers the original port list. Mirrors the
        // `synthdef_outputs` registration shape exactly.
        let mut sd = vibelang_dsp::SynthDef::new("input_round_trip".to_string());
        sd.input("audio_mono".to_string(), 1)
            .expect("ar mono input");
        sd.input("audio_stereo".to_string(), 2)
            .expect("ar stereo input");
        // No `.input_kr()` builder method exists yet (P1.1 only ships
        // ar-rate `.input`); construct the kr descriptor directly. The
        // round-trip is over the State-side registration shape, not the
        // builder surface.
        sd.inputs.push(InputPort {
            name: "cv".to_string(),
            channels: 1,
            rate: vibelang_dsp::PortRate::Kr,
        });
        let inputs = sd.inputs.clone();

        let mut state = State::default();
        assert!(
            state.synthdef_inputs("input_round_trip").is_empty(),
            "default state has no inputs registered"
        );

        state
            .synthdef_inputs
            .insert("input_round_trip".to_string(), inputs.clone());
        let recovered = state.synthdef_inputs("input_round_trip");
        assert_eq!(recovered, inputs, "recovered ports must match originals");
        assert_eq!(recovered.len(), 3, "two ar + one kr input expected");
        assert_eq!(recovered[0].channels, 1);
        assert_eq!(recovered[0].rate, vibelang_dsp::PortRate::Ar);
        assert_eq!(recovered[1].channels, 2);
        assert_eq!(recovered[1].rate, vibelang_dsp::PortRate::Ar);
        assert_eq!(recovered[2].rate, vibelang_dsp::PortRate::Kr);

        assert!(
            state.synthdef_inputs("unregistered").is_empty(),
            "unregistered synthdefs resolve to no inputs"
        );
    }
}
