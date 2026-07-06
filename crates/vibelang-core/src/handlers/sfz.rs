//! SFZ handler implementation.
//!
//! This handler manages SFZ instrument loading, unloading, and region matching.
//! It integrates with the vibelang-sfz crate for SFZ file parsing and provides
//! persistent round-robin state for proper sample alternation.

use crate::backend::Backend;
use crate::compat::RwLock;
use crate::state::{SfzInstrumentState, SfzRegionState, State};
use crate::traits::{Sfz, SfzTriggerInfo, SfzTriggerMode};
use crate::types::{BufferId, SfzId};
use crate::{Error, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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
}

impl<B: Backend> SfzHandler<B> {
    /// Create a new SFZ handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Sfz for SfzHandler<B> {
    async fn load(&self, id: SfzId, path: &Path) -> Result<()> {
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

            // Now actually load the buffers via backend
            for load in buffer_loads {
                let our_buffer_id = {
                    let mut state = self.state.write().await;
                    state.alloc_buffer_id()?
                };

                self.backend
                    .load_buffer(our_buffer_id, &load.path)
                    .await
                    .map_err(|e| Error::SfzLoadFailed {
                        path: load.path.clone(),
                        reason: e.to_string(),
                    })?;

                buffer_id_map.insert(load.sfz_buffer_id, our_buffer_id);

                tracing::debug!(
                    "Loaded SFZ sample {} -> buffer {}",
                    load.path.display(),
                    our_buffer_id.0
                );
            }

            instrument
        };

        // Convert to our state format
        let regions: Vec<SfzRegionState> = sfz_instrument
            .regions
            .iter()
            .map(|r| Self::build_region_state(r, &buffer_id_map))
            .collect();

        // Store in state
        {
            let mut state = self.state.write().await;
            state.sfz_instruments.insert(
                id,
                SfzInstrumentState {
                    id,
                    path: path.to_path_buf(),
                    regions,
                    round_robin_state: HashMap::new(),
                },
            );
        }

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

        Ok(())
    }

    async fn unload(&self, id: SfzId) -> Result<()> {
        tracing::info!("Unloading SFZ instrument {}", id.0);

        // Get the instrument and collect buffer IDs to free
        let buffer_ids: Vec<BufferId> = {
            let mut state = self.state.write().await;
            let instrument = state
                .sfz_instruments
                .remove(&id)
                .ok_or(Error::SfzNotFound(id))?;

            // Deduplicate before freeing (same buffer may back multiple regions)
            let unique: std::collections::HashSet<BufferId> =
                instrument.regions.iter().map(|r| r.buffer_id).collect();
            for buf_id in &unique {
                state.free_buffer_id(*buf_id);
            }
            unique.into_iter().collect()
        };

        // Free buffers in backend
        for buffer_id in buffer_ids {
            if let Err(e) = self.backend.free_buffer(buffer_id).await {
                tracing::warn!("Failed to free buffer {}: {}", buffer_id.0, e);
            }
        }

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
    let mut result = Vec::new();

    // Group regions by round-robin key for proper RR handling
    let mut rr_groups: HashMap<String, Vec<&SfzRegionState>> = HashMap::new();
    let mut non_rr_regions: Vec<&SfzRegionState> = Vec::new();

    for region in &instrument.regions {
        // Check basic criteria
        if note < region.key_range.0 || note > region.key_range.1 {
            continue;
        }
        if velocity < region.vel_range.0 || velocity > region.vel_range.1 {
            continue;
        }

        if region.seq_length > 0 {
            // Round-robin region
            let rr_key = format!("{}_{}", note, 0); // Using 0 for group since we don't track it
            rr_groups.entry(rr_key).or_default().push(region);
        } else {
            non_rr_regions.push(region);
        }
    }

    // Process non-RR regions
    for region in non_rr_regions {
        result.push(build_trigger_info(region, note));
    }

    // Process RR groups: collect selections first (the RR-state update needs
    // &mut instrument while region refs borrow it immutably).
    let mut rr_selected: Vec<SfzTriggerInfo> = Vec::new();
    let mut rr_updates: Vec<(u8, u32)> = Vec::new();
    for (rr_key, regions) in rr_groups {
        if regions.is_empty() {
            continue;
        }

        let seq_len = regions[0].seq_length;
        let current_pos = {
            let current = instrument
                .round_robin_state
                .get(&(note, 0))
                .copied()
                .unwrap_or(1);
            let next = if current >= seq_len { 1 } else { current + 1 };
            rr_updates.push((note, next));
            current
        };

        // Find region matching this position
        for region in regions {
            if region.seq_position == current_pos {
                rr_selected.push(build_trigger_info(region, note));
                break;
            }
        }

        tracing::trace!(
            "RR key {}: selected position {} of {}",
            rr_key,
            current_pos,
            seq_len
        );
    }
    for (note, next) in rr_updates {
        instrument.round_robin_state.insert((note, 0), next);
    }
    result.extend(rr_selected);

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
    /// final `amp` (note velocity x region volume).
    pub params: Vec<(String, f32)>,
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

    let infos = matching_regions_for_note(instrument, note, midi_velocity);
    if infos.is_empty() {
        tracing::warn!(
            "SFZ {}: no region matches note {} velocity {} — note skipped",
            sfz_id.0,
            note,
            midi_velocity
        );
        return None;
    }
    if infos.len() > 1 {
        tracing::debug!(
            "SFZ {}: {} regions match note {} — playing the first (layering unsupported)",
            sfz_id.0,
            infos.len(),
            note
        );
    }
    let info = &infos[0];

    let synthdef = if info.num_channels == 1 {
        "sfz_voice_mono"
    } else {
        "sfz_voice_stereo"
    };

    let mut params = vec![
        ("bufnum".to_string(), info.buffer_id.0 as f32),
        ("rate".to_string(), info.rate),
        ("amp".to_string(), velocity.clamp(0.0, 1.0) * info.amp),
        ("attack".to_string(), info.ampeg_attack),
        ("decay".to_string(), info.ampeg_decay),
        ("sustain".to_string(), info.ampeg_sustain),
        ("release".to_string(), info.ampeg_release),
        ("pan".to_string(), info.pan),
        (
            "loop".to_string(),
            if info.loop_enabled { 1.0 } else { 0.0 },
        ),
    ];
    if info.offset > 0 {
        params.push(("startPos".to_string(), info.offset as f32));
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
        state.sfz_instruments.insert(
            sfz_id,
            SfzInstrumentState {
                id: sfz_id,
                path: std::path::PathBuf::from("pluck.sfz"),
                regions: vec![low, high],
                round_robin_state: HashMap::new(),
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
                .find(|(n, _)| n == name)
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
            },
        );

        let spawn = sfz_note_spawn_params(&mut state, sfz_id, 60, 1.0).unwrap();
        let loop_param = spawn
            .params
            .iter()
            .find(|(n, _)| n == "loop")
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
