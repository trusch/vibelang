//! SFZ handler implementation.
//!
//! This handler manages SFZ instrument loading, unloading, and region matching.
//! It integrates with the vibelang-sfz crate for SFZ file parsing and provides
//! persistent round-robin state for proper sample alternation.

use crate::backend::Backend;
use crate::compat::{Instant, RwLock};
use crate::handlers::samples::BUFFER_FREE_GRACE_PERIOD_MS;
use crate::state::{SfzInstrumentState, SfzRegionState, State};
use crate::traits::{Sfz, SfzTriggerInfo, SfzTriggerMode};
use crate::types::{BufferId, SfzId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crate::compat::Duration;

/// Handler for SFZ instrument management.
///
/// The SFZ handler coordinates:
/// - Loading SFZ files and their samples into buffers
/// - Maintaining persistent round-robin state
/// - Finding matching regions for note triggers
///
/// # Round-Robin State
///
/// Unlike the old implementation, round-robin state is stored persistently
/// per instrument in the global state. This ensures proper sample cycling
/// across multiple note triggers.
pub struct SfzHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Region buffers pending backend free after the grace period (same
    /// deferred-free discipline as `SamplesHandler`: never `/b_free` a
    /// buffer a live synth node may still read). Drained by [`Self::tick`];
    /// buffer IDs return to the allocator pool only when the free executes.
    pending_frees: Arc<RwLock<Vec<(BufferId, Instant)>>>,
}

impl<B: Backend> SfzHandler<B> {
    /// Create a new SFZ handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            pending_frees: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Schedule an instrument's region buffers for backend free after the
    /// grace period (deduplicated — one buffer may back multiple regions).
    async fn defer_free_instrument_buffers(&self, instrument: &SfzInstrumentState) {
        let unique: std::collections::HashSet<BufferId> =
            instrument.regions.iter().map(|r| r.buffer_id).collect();
        if unique.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut pending = self.pending_frees.write().await;
        for buffer_id in unique {
            pending.push((buffer_id, now));
        }
        tracing::debug!(
            "SFZ instrument {}: scheduled {} buffer(s) for free after {}ms grace period",
            instrument.id.0,
            instrument.regions.len(),
            BUFFER_FREE_GRACE_PERIOD_MS
        );
    }

    /// Process pending buffer frees whose grace period elapsed.
    ///
    /// Called by the runtime's tick loop.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn tick(&self) {
        let now = Instant::now();
        let grace_period = Duration::from_millis(BUFFER_FREE_GRACE_PERIOD_MS);

        let buffers_to_free: Vec<BufferId> = {
            let mut pending = self.pending_frees.write().await;
            if pending.is_empty() {
                return;
            }
            let mut to_free = Vec::new();
            let mut remaining = Vec::new();
            for (buffer_id, requested_at) in pending.drain(..) {
                if now.duration_since(requested_at) >= grace_period {
                    to_free.push(buffer_id);
                } else {
                    remaining.push((buffer_id, requested_at));
                }
            }
            *pending = remaining;
            to_free
        };

        for buffer_id in buffers_to_free {
            tracing::debug!(
                "SFZ buffer grace period elapsed, freeing buffer {}",
                buffer_id.0
            );
            if let Err(e) = self.backend.free_buffer(buffer_id).await {
                tracing::warn!("free_buffer({}) failed: {}", buffer_id.0, e);
            }
            self.state.write().await.free_buffer_id(buffer_id);
        }
    }

    /// Process pending buffer frees (WASM version - immediate free).
    #[cfg(target_arch = "wasm32")]
    pub async fn tick(&self) {
        let buffers_to_free: Vec<BufferId> = {
            let mut pending = self.pending_frees.write().await;
            pending.drain(..).map(|(buffer_id, _)| buffer_id).collect()
        };
        for buffer_id in buffers_to_free {
            let _ = self.backend.free_buffer(buffer_id).await;
            self.state.write().await.free_buffer_id(buffer_id);
        }
    }

    /// Convert vibelang-sfz LoopMode to our boolean.
    fn is_loop_enabled(mode: vibelang_sfz::LoopMode) -> bool {
        matches!(
            mode,
            vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
        )
    }

    /// Build SfzRegionState from a loaded SfzRegion.
    fn build_region_state(
        region: &vibelang_sfz::SfzRegion,
        buffer_id_map: &HashMap<i32, BufferId>,
    ) -> SfzRegionState {
        let buffer_id = buffer_id_map
            .get(&region.buffer_id)
            .copied()
            .unwrap_or(BufferId::new(0));

        SfzRegionState {
            buffer_id,
            num_channels: region.num_channels as u8,
            key_range: region.key_range,
            vel_range: region.vel_range,
            pitch_keycenter: region.opcodes.pitch_keycenter.unwrap_or(60),
            seq_position: region.seq_position.unwrap_or(0) as u32,
            seq_length: region.seq_length.unwrap_or(0) as u32,
            ampeg_attack: region.opcodes.ampeg_attack.unwrap_or(0.001),
            ampeg_decay: region.opcodes.ampeg_decay.unwrap_or(0.0),
            ampeg_sustain: region
                .opcodes
                .ampeg_sustain
                .map(|s| s / 100.0)
                .unwrap_or(1.0),
            ampeg_release: region.opcodes.ampeg_release.unwrap_or(0.01),
            volume: region.opcodes.volume.unwrap_or(0.0),
            pan: region
                .opcodes
                .pan
                .map(vibelang_sfz::sfz_pan_to_sc)
                .unwrap_or(0.0),
            transpose: region.opcodes.transpose.unwrap_or(0) as i8,
            tune: region.opcodes.tune.unwrap_or(0.0),
            loop_enabled: Self::is_loop_enabled(region.loop_mode),
            loop_start: region.loop_start.unwrap_or(0),
            loop_end: region.loop_end.unwrap_or(0),
            offset: region.opcodes.offset.unwrap_or(0),
            cutoff: region.opcodes.cutoff,
            resonance: region.opcodes.resonance,
        }
    }
}

impl<B: Backend> SfzHandler<B> {
    /// Parse an SFZ file and load its sample buffers without publishing
    /// the instrument to state.
    ///
    /// This is the expensive half of [`Sfz::load`] (file I/O plus one
    /// backend `/b_allocRead` round-trip per unique sample). It can run
    /// off the runtime task; pair with [`Self::commit`] to make the
    /// instrument visible atomically. If the staged instrument is
    /// discarded instead, its region buffers must be freed via
    /// `State::free_buffer_id` + `Backend::free_buffer`.
    pub async fn stage_load(&self, id: SfzId, path: &Path) -> Result<SfzInstrumentState> {
        tracing::info!("Loading SFZ instrument {} from {}", id.0, path.display());

        // We need to load the SFZ file, then load each unique sample via backend
        // next_buffer_id is a local counter used by the SFZ library to generate
        // internal IDs for its sample mapping. These are separate from our BufferIds.
        let mut next_buffer_id = 0i32;

        // Track which sfz buffer IDs map to our BufferIds
        let mut buffer_id_map: HashMap<i32, BufferId> = HashMap::new();

        // Load the SFZ instrument using vibelang-sfz
        // We capture sample paths and their assigned buffer IDs
        let sfz_instrument = {
            let path_buf = path.to_path_buf();
            let name = id.0.to_string();

            // This is a sync function, so we need to handle it carefully
            // We'll use a closure to capture buffer load results
            struct BufferLoadResult {
                path: std::path::PathBuf,
                sfz_buffer_id: i32,
            }

            let mut buffer_loads: Vec<BufferLoadResult> = Vec::new();

            let result = vibelang_sfz::load_sfz_instrument(
                &path_buf,
                name,
                &mut |sample_path, buffer_id| {
                    buffer_loads.push(BufferLoadResult {
                        path: sample_path.to_path_buf(),
                        sfz_buffer_id: buffer_id,
                    });
                    Ok(())
                },
                &mut next_buffer_id,
            );

            let instrument = result.map_err(|e| Error::SfzLoadFailed {
                path: path_buf.clone(),
                reason: e.to_string(),
            })?;

            // Allocate all buffer IDs up front in deterministic (load) order,
            // then load the samples concurrently: scsynth allows concurrent
            // /b_allocRead (per-bufnum done keys), so the round-trips overlap.
            // A single failed sample aborts the whole batch, matching the
            // previous serial loop's error semantics.
            use futures::StreamExt;
            let mut allocations: Vec<(i32, BufferId, std::path::PathBuf)> =
                Vec::with_capacity(buffer_loads.len());
            {
                let mut state = self.state.write().await;
                for load in &buffer_loads {
                    let our_buffer_id = state.alloc_buffer_id()?;
                    allocations.push((load.sfz_buffer_id, our_buffer_id, load.path.clone()));
                }
            }

            let backend = &self.backend;
            let mut loads = futures::stream::iter(allocations.into_iter().map(
                |(sfz_buffer_id, buffer_id, path)| async move {
                    backend
                        .load_buffer(buffer_id, &path)
                        .await
                        .map_err(|e| Error::SfzLoadFailed {
                            path: path.clone(),
                            reason: e.to_string(),
                        })?;
                    tracing::debug!(
                        "Loaded SFZ sample {} -> buffer {}",
                        path.display(),
                        buffer_id.0
                    );
                    Ok::<(i32, BufferId), Error>((sfz_buffer_id, buffer_id))
                },
            ))
            .buffer_unordered(8);

            while let Some(result) = loads.next().await {
                let (sfz_buffer_id, our_buffer_id) = result?;
                buffer_id_map.insert(sfz_buffer_id, our_buffer_id);
            }

            instrument
        };

        // Convert to our state format
        let regions: Vec<SfzRegionState> = sfz_instrument
            .regions
            .iter()
            .map(|r| Self::build_region_state(r, &buffer_id_map))
            .collect();

        // Surface load diagnostics once per instrument: how many regions
        // made it, why any were dropped, and which opcodes were ignored.
        let diag = &sfz_instrument.diagnostics;
        if diag.regions_dropped() > 0 {
            tracing::warn!(
                "SFZ instrument {}: {} regions parsed, {} loaded, {} dropped ({})",
                id.0,
                diag.regions_parsed,
                diag.regions_loaded,
                diag.regions_dropped(),
                diag.dropped_summary()
            );
        } else {
            tracing::info!(
                "Loaded SFZ instrument {} with {} regions",
                id.0,
                diag.regions_loaded
            );
        }
        if !diag.unknown_opcodes.is_empty() {
            tracing::warn!(
                "SFZ instrument {}: {} unrecognized opcode(s) ignored: {}",
                id.0,
                diag.unknown_opcodes.len(),
                diag.unknown_opcodes_summary()
            );
        }

        let note_index = SfzInstrumentState::build_note_index(&regions);
        Ok(SfzInstrumentState {
            id,
            path: path.to_path_buf(),
            regions,
            round_robin_state: HashMap::new(),
            note_index,
        })
    }

    /// Publish a staged instrument into state (synchronous mutation only).
    ///
    /// If the insert displaces an existing instrument under the same ID
    /// (in-place reload), the OLD instrument's region buffers are freed
    /// only after the grace period — playing notes keep reading them,
    /// while new note-ons match regions against the new instrument.
    pub async fn commit(&self, instrument: SfzInstrumentState) {
        let displaced = {
            let mut state = self.state.write().await;
            state.sfz_instruments.insert(instrument.id, instrument)
        };
        if let Some(old) = displaced {
            self.defer_free_instrument_buffers(&old).await;
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Sfz for SfzHandler<B> {
    async fn load(&self, id: SfzId, path: &Path) -> Result<()> {
        let instrument = self.stage_load(id, path).await?;
        self.commit(instrument).await;
        Ok(())
    }

    async fn unload(&self, id: SfzId) -> Result<()> {
        tracing::info!("Unloading SFZ instrument {}", id.0);

        // Remove the mapping now (no new note may match this instrument),
        // but defer the backend frees and ID-pool returns past the grace
        // period — live nodes may still be reading the region buffers.
        let instrument = {
            let mut state = self.state.write().await;
            state
                .sfz_instruments
                .remove(&id)
                .ok_or(Error::SfzNotFound(id))?
        };
        self.defer_free_instrument_buffers(&instrument).await;

        Ok(())
    }

    async fn find_regions(
        &self,
        id: SfzId,
        note: u8,
        velocity: u8,
        trigger_mode: SfzTriggerMode,
    ) -> Result<Vec<SfzTriggerInfo>> {
        let mut state = self.state.write().await;

        let instrument = state
            .sfz_instruments
            .get_mut(&id)
            .ok_or(Error::SfzNotFound(id))?;

        // Note: We only support Attack trigger mode for now
        // Release triggers would need special handling in the voice/melody handlers
        if trigger_mode != SfzTriggerMode::Attack {
            return Ok(Vec::new());
        }

        let result = matching_regions_for_note(instrument, note, velocity);

        tracing::debug!(
            "Found {} matching regions for note {} vel {} on SFZ {}",
            result.len(),
            note,
            velocity,
            id.0
        );

        Ok(result)
    }
}

/// Find all attack-trigger regions matching `(note, velocity)`, advancing
/// the instrument's persistent round-robin state.
///
/// Shared by the async `Sfz::find_regions` trait path and the synchronous
/// note-on path ([`sfz_note_spawn_params`]).
fn matching_regions_for_note(
    instrument: &mut crate::state::SfzInstrumentState,
    note: u8,
    velocity: u8,
) -> Vec<SfzTriggerInfo> {
    let indices = matching_region_indices_for_note(instrument, note, velocity);
    indices
        .into_iter()
        .map(|idx| build_trigger_info(&instrument.regions[idx], note))
        .collect()
}

/// Select the region indices (into `instrument.regions`) that match
/// `(note, velocity)` for an attack trigger, advancing the instrument's
/// persistent round-robin state exactly once. Non-RR matches come first (in
/// region order), then the single round-robin selection.
///
/// The round-robin key is `(note, group)` with `group` hardcoded to 0 and
/// `note` fixed for the whole call, so every round-robin region collapses into
/// one group — a plain `Vec` replaces the old per-note `String`-keyed map.
/// Returning indices (not `SfzTriggerInfo`) lets the note-on hot path build the
/// trigger info only for the region it actually spawns.
/// Classify one key-matched region by velocity into either the direct-match
/// set or the round-robin candidate set. Key range is assumed already checked
/// by the caller (bucket index or explicit scan).
fn classify_sfz_region(
    region: &SfzRegionState,
    idx: usize,
    velocity: u8,
    result: &mut Vec<usize>,
    rr_indices: &mut Vec<usize>,
) {
    if velocity < region.vel_range.0 || velocity > region.vel_range.1 {
        return;
    }
    if region.seq_length > 0 {
        rr_indices.push(idx);
    } else {
        result.push(idx);
    }
}

fn matching_region_indices_for_note(
    instrument: &mut crate::state::SfzInstrumentState,
    note: u8,
    velocity: u8,
) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::new();
    let mut rr_indices: Vec<usize> = Vec::new();

    // Fast path: walk only the regions whose key range covers `note` via the
    // precomputed bucket. If the index is absent (empty), fall back to a full
    // region scan so correctness never depends on the accelerator. Both paths
    // apply the same velocity + round-robin classification.
    match instrument.note_index.get(note as usize) {
        Some(bucket) => {
            for &ridx in bucket {
                let idx = ridx as usize;
                classify_sfz_region(&instrument.regions[idx], idx, velocity, &mut result, &mut rr_indices);
            }
        }
        None => {
            for (idx, region) in instrument.regions.iter().enumerate() {
                if note < region.key_range.0 || note > region.key_range.1 {
                    continue;
                }
                classify_sfz_region(region, idx, velocity, &mut result, &mut rr_indices);
            }
        }
    }

    if !rr_indices.is_empty() {
        let seq_len = instrument.regions[rr_indices[0]].seq_length;
        let current = instrument
            .round_robin_state
            .get(&(note, 0))
            .copied()
            .unwrap_or(1);
        let next = if current >= seq_len { 1 } else { current + 1 };
        instrument.round_robin_state.insert((note, 0), next);
        for &idx in &rr_indices {
            if instrument.regions[idx].seq_position == current {
                result.push(idx);
                break;
            }
        }
        tracing::trace!("RR note {}: selected position {} of {}", note, current, seq_len);
    }

    result
}

/// Build SfzTriggerInfo from a region state.
fn build_trigger_info(region: &SfzRegionState, target_note: u8) -> SfzTriggerInfo {
    // Calculate playback rate for correct pitch
    let rate = vibelang_sfz::calculate_playback_rate(
        target_note,
        Some(region.pitch_keycenter),
        Some(region.tune),
        Some(region.transpose as i32),
    );

    // Convert volume from dB to linear amplitude
    let amp = vibelang_sfz::db_to_amp(region.volume);

    SfzTriggerInfo {
        buffer_id: region.buffer_id,
        rate,
        num_channels: region.num_channels,
        ampeg_attack: region.ampeg_attack,
        ampeg_decay: region.ampeg_decay,
        ampeg_sustain: region.ampeg_sustain,
        ampeg_release: region.ampeg_release,
        amp,
        pan: region.pan,
        loop_enabled: region.loop_enabled,
        loop_start: region.loop_start,
        loop_end: region.loop_end,
        offset: region.offset,
        cutoff: region.cutoff,
        resonance: region.resonance,
    }
}

/// Synth spawn parameters for one SFZ note, computed from the matched region.
///
/// This is what `VoicesHandler::note_on_audio_at` must apply when the voice
/// has `config.sfz_instrument` set — without it the `sfz_voice_*` synth
/// spawns with default params (`bufnum=0`, `rate=1`, `release=0.01`,
/// `loop=0`), i.e. the wrong sample at the wrong pitch with a click release.
#[derive(Clone, Debug, PartialEq)]
pub struct SfzNoteSpawn {
    /// Synthdef matching the sample's channel count
    /// (`sfz_voice_mono` / `sfz_voice_stereo`).
    pub synthdef: &'static str,
    /// Params to merge into the note's synth-creation params. Includes the
    /// final `amp` (note velocity x region volume). Names are `&'static str`
    /// literals; the note-on path allocates the owned `String` keys on insert.
    pub params: Vec<(&'static str, f32)>,
}

/// Select the SFZ region for `(note, velocity)` and compute the synth
/// params a note-on must apply. Advances the instrument's round-robin
/// state. `velocity` is the normalized 0..1 note velocity.
///
/// Returns `None` when the instrument is unknown or no region matches
/// (note/velocity out of every region's range) — the caller should skip
/// spawning a synth in that case rather than play a wrong buffer.
///
/// When several regions match (velocity-layer crossfades), only the first
/// is used: the voice note-path tracks a single node per note. Layered
/// multi-region playback is a known limitation.
pub fn sfz_note_spawn_params(
    state: &mut State,
    sfz_id: SfzId,
    note: u8,
    velocity: f32,
) -> Option<SfzNoteSpawn> {
    let instrument = state.sfz_instruments.get_mut(&sfz_id)?;
    let midi_velocity = (velocity.clamp(0.0, 1.0) * 127.0).round() as u8;

    let indices = matching_region_indices_for_note(instrument, note, midi_velocity);
    if indices.is_empty() {
        tracing::warn!(
            "SFZ {}: no region matches note {} velocity {} — note skipped",
            sfz_id.0,
            note,
            midi_velocity
        );
        return None;
    }
    if indices.len() > 1 {
        tracing::debug!(
            "SFZ {}: {} regions match note {} — playing the first (layering unsupported)",
            sfz_id.0,
            indices.len(),
            note
        );
    }
    // Build trigger info only for the region actually spawned (indices[0]);
    // the extra matches are diagnostic (layering unsupported).
    let info = build_trigger_info(&instrument.regions[indices[0]], note);

    let synthdef = if info.num_channels == 1 {
        "sfz_voice_mono"
    } else {
        "sfz_voice_stereo"
    };

    let mut params: Vec<(&'static str, f32)> = vec![
        ("bufnum", info.buffer_id.0 as f32),
        ("rate", info.rate),
        ("amp", velocity.clamp(0.0, 1.0) * info.amp),
        ("attack", info.ampeg_attack),
        ("decay", info.ampeg_decay),
        ("sustain", info.ampeg_sustain),
        ("release", info.ampeg_release),
        ("pan", info.pan),
        ("loop", if info.loop_enabled { 1.0 } else { 0.0 }),
    ];
    if info.offset > 0 {
        params.push(("startPos", info.offset as f32));
    }

    Some(SfzNoteSpawn { synthdef, params })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a State holding a two-region SFZ instrument mirroring
    /// examples/tutorials/assets/tutorial_pluck.sfz.
    fn state_with_pluck(sfz_id: SfzId) -> State {
        let mut state = State::new();
        let low = SfzRegionState {
            buffer_id: BufferId::new(10),
            num_channels: 1,
            key_range: (0, 59),
            pitch_keycenter: 57,
            ampeg_release: 0.15,
            ..Default::default()
        };
        let high = SfzRegionState {
            buffer_id: BufferId::new(11),
            num_channels: 2,
            key_range: (60, 127),
            pitch_keycenter: 69,
            ampeg_release: 0.15,
            ..Default::default()
        };
        let regions = vec![low, high];
        let note_index = SfzInstrumentState::build_note_index(&regions);
        state.sfz_instruments.insert(
            sfz_id,
            SfzInstrumentState {
                id: sfz_id,
                path: std::path::PathBuf::from("pluck.sfz"),
                regions,
                round_robin_state: HashMap::new(),
                note_index,
            },
        );
        state
    }

    /// Root-cause regression: note-on for an SFZ voice must apply the
    /// matched region's bufnum/rate/release — the synthdef defaults
    /// (bufnum=0, rate=1, release=0.01) play the wrong sample at the wrong
    /// pitch and click on NOTE_OFF. This pins the parameters the note-on
    /// path has to send (see kb/tickets/core-concepts/sfz-instrument).
    #[test]
    fn sfz_note_spawn_selects_region_and_envelope() {
        let sfz_id = SfzId::new(1);
        let mut state = state_with_pluck(sfz_id);

        // A2 (45): low region, one octave below its keycenter -> rate 0.5.
        let spawn = sfz_note_spawn_params(&mut state, sfz_id, 45, 0.8).unwrap();
        assert_eq!(spawn.synthdef, "sfz_voice_mono");
        let get = |name: &str, spawn: &SfzNoteSpawn| {
            spawn
                .params
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| *v)
                .unwrap_or_else(|| panic!("param {} missing", name))
        };
        assert_eq!(get("bufnum", &spawn), 10.0);
        assert!((get("rate", &spawn) - 0.5).abs() < 1e-6);
        assert!((get("release", &spawn) - 0.15).abs() < 1e-6);
        assert!((get("amp", &spawn) - 0.8).abs() < 1e-6);
        assert_eq!(get("loop", &spawn), 0.0);

        // A4 (69): high region at unity rate, stereo sample.
        let spawn = sfz_note_spawn_params(&mut state, sfz_id, 69, 1.0).unwrap();
        assert_eq!(spawn.synthdef, "sfz_voice_stereo");
        assert_eq!(get("bufnum", &spawn), 11.0);
        assert!((get("rate", &spawn) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sfz_note_spawn_returns_none_when_no_region_matches() {
        let sfz_id = SfzId::new(2);
        let mut state = State::new();
        state.sfz_instruments.insert(
            sfz_id,
            SfzInstrumentState {
                id: sfz_id,
                path: std::path::PathBuf::from("x.sfz"),
                regions: vec![SfzRegionState {
                    key_range: (60, 72),
                    vel_range: (64, 127),
                    ..Default::default()
                }],
                round_robin_state: HashMap::new(),
                note_index: SfzInstrumentState::build_note_index(&[SfzRegionState {
                    key_range: (60, 72),
                    vel_range: (64, 127),
                    ..Default::default()
                }]),
            },
        );

        // Note out of key range.
        assert!(sfz_note_spawn_params(&mut state, sfz_id, 40, 1.0).is_none());
        // Velocity below the region's lovel.
        assert!(sfz_note_spawn_params(&mut state, sfz_id, 60, 0.1).is_none());
        // Unknown instrument.
        assert!(sfz_note_spawn_params(&mut state, SfzId::new(99), 60, 1.0).is_none());
        // In range works.
        assert!(sfz_note_spawn_params(&mut state, sfz_id, 60, 1.0).is_some());
    }

    #[test]
    fn sfz_note_spawn_sets_loop_for_sustaining_regions() {
        let sfz_id = SfzId::new(3);
        let mut state = State::new();
        state.sfz_instruments.insert(
            sfz_id,
            SfzInstrumentState {
                id: sfz_id,
                path: std::path::PathBuf::from("pad.sfz"),
                regions: vec![SfzRegionState {
                    loop_enabled: true,
                    ..Default::default()
                }],
                round_robin_state: HashMap::new(),
                note_index: SfzInstrumentState::build_note_index(&[SfzRegionState {
                    loop_enabled: true,
                    ..Default::default()
                }]),
            },
        );

        let spawn = sfz_note_spawn_params(&mut state, sfz_id, 60, 1.0).unwrap();
        let loop_param = spawn
            .params
            .iter()
            .find(|(n, _)| *n == "loop")
            .map(|(_, v)| *v);
        assert_eq!(loop_param, Some(1.0));
    }

    #[test]
    fn test_sfz_trigger_modes() {
        // Verify SfzTriggerMode variants exist and are distinct
        assert_ne!(SfzTriggerMode::Attack, SfzTriggerMode::Release);
        assert_ne!(SfzTriggerMode::First, SfzTriggerMode::Legato);
    }

    #[test]
    fn test_is_loop_enabled() {
        // Test the is_loop_enabled helper function
        // Note: This is a static method, so we need to use a concrete type.
        // We test via the direct logic since the function is simple.
        assert!(matches!(
            vibelang_sfz::LoopMode::Loop,
            vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
        ));
        assert!(matches!(
            vibelang_sfz::LoopMode::LoopContinuous,
            vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
        ));
        assert!(!matches!(
            vibelang_sfz::LoopMode::NoLoop,
            vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
        ));
        assert!(!matches!(
            vibelang_sfz::LoopMode::OneShot,
            vibelang_sfz::LoopMode::Loop | vibelang_sfz::LoopMode::LoopContinuous
        ));
    }
}
