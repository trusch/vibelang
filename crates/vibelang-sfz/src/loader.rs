//! SFZ file loading and sample buffer management.

use crate::parser::{parse_sfz_file, LoopMode, SfzFile, SfzSection, TriggerMode};
use crate::types::*;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Callback type for loading a sample buffer.
///
/// The callback receives:
/// - The path to the sample file
/// - The buffer ID to use
///
/// It should return Ok(()) on success or an error message.
pub type BufferLoadCallback<'a> = &'a mut dyn FnMut(&Path, i32) -> Result<()>;

/// Load an SFZ instrument from a file.
///
/// This function parses the SFZ file and loads all samples using the provided
/// buffer load callback. The callback is responsible for actually loading the
/// audio data into the audio backend.
///
/// # Arguments
///
/// * `sfz_path` - Path to the SFZ file
/// * `name` - Name for this instrument
/// * `load_buffer` - Callback to load a sample into a buffer
/// * `next_buffer_id` - Mutable reference to the next available buffer ID
///
/// # Example
///
/// ```ignore
/// let mut next_buffer_id = 100;
/// let instrument = load_sfz_instrument(
///     "path/to/instrument.sfz",
///     "my_bass".to_string(),
///     &mut |path, buffer_id| {
///         // Load the sample at `path` into buffer `buffer_id`
///         sc.b_alloc_read(buffer_id, path)
///     },
///     &mut next_buffer_id,
/// )?;
/// ```
pub fn load_sfz_instrument<P: AsRef<Path>>(
    sfz_path: P,
    name: String,
    load_buffer: BufferLoadCallback,
    next_buffer_id: &mut i32,
) -> Result<SfzInstrument> {
    let sfz_path = sfz_path.as_ref();

    // Parse the SFZ file
    let sfz_file = parse_sfz_file(sfz_path)
        .with_context(|| format!("Failed to parse SFZ file: {}", sfz_path.display()))?;

    log::info!(
        "Loading SFZ instrument '{}' from {}",
        name,
        sfz_path.display()
    );
    log::info!("Found {} regions", sfz_file.regions.len());

    // Extract global and control opcodes
    let global_opcodes = extract_opcodes_from_section(sfz_file.global.as_ref());
    let control_opcodes = extract_opcodes_from_section(sfz_file.control.as_ref());

    // Load all regions
    let mut regions = Vec::new();
    let mut sample_cache: HashMap<PathBuf, i32> = HashMap::new();

    for sfz_region in &sfz_file.regions {
        match load_sfz_region(
            &sfz_file,
            sfz_region,
            &global_opcodes,
            load_buffer,
            next_buffer_id,
            &mut sample_cache,
        ) {
            Ok(region) => regions.push(region),
            Err(e) => {
                log::warn!("Failed to load region: {}", e);
                // Continue loading other regions
            }
        }
    }

    log::info!(
        "Successfully loaded {} regions for instrument '{}'",
        regions.len(),
        name
    );

    Ok(SfzInstrument {
        name,
        source_file: sfz_path.to_path_buf(),
        regions,
        global_opcodes,
        control_opcodes,
    })
}

/// Load a single SFZ region.
fn load_sfz_region(
    sfz_file: &SfzFile,
    sfz_region: &SfzSection,
    global_opcodes: &HashMap<String, String>,
    load_buffer: BufferLoadCallback,
    next_buffer_id: &mut i32,
    sample_cache: &mut HashMap<PathBuf, i32>,
) -> Result<SfzRegion> {
    // Get the sample path
    let sample_path = sfz_file
        .resolve_absolute_sample_path(sfz_region)
        .context("No sample path defined in region")?;

    // Load the sample into a buffer (or reuse if already loaded)
    let buffer_id = if let Some(&cached_id) = sample_cache.get(&sample_path) {
        log::debug!(
            "Reusing buffer {} for sample: {}",
            cached_id,
            sample_path.display()
        );
        cached_id
    } else {
        let buffer_id = *next_buffer_id;
        *next_buffer_id += 1;

        log::debug!(
            "Loading sample into buffer {}: {}",
            buffer_id,
            sample_path.display()
        );

        // Load the sample via callback
        load_buffer(sample_path.as_path(), buffer_id)
            .with_context(|| format!("Failed to load sample: {}", sample_path.display()))?;

        sample_cache.insert(sample_path.clone(), buffer_id);
        buffer_id
    };

    // Read WAV metadata for duration calculation
    let (buffer_frames, sample_rate) = read_wav_info(&sample_path)
        .unwrap_or((44100, 44100.0)); // Default to 1 second at 44.1kHz if reading fails

    // Extract region parameters
    let opcodes = parse_region_opcodes(sfz_region, global_opcodes)?;

    // Extract key and velocity ranges
    let key_range = extract_key_range(sfz_region)?;
    let vel_range = extract_vel_range(sfz_region);

    // Extract trigger mode
    let trigger = extract_trigger_mode(sfz_region);

    // Extract loop mode and points
    let loop_mode = extract_loop_mode(sfz_region);
    let loop_start = extract_opcode_u32(sfz_region, "loop_start");
    let loop_end = extract_opcode_u32(sfz_region, "loop_end");

    // Extract group information
    let group = extract_opcode_i64(sfz_region, "group");
    let off_by = extract_opcode_i64(sfz_region, "off_by");

    // Extract round-robin information
    let seq_position = extract_opcode_i64(sfz_region, "seq_position");
    let seq_length = extract_opcode_i64(sfz_region, "seq_length");

    // Detect number of channels from WAV file
    let num_channels = detect_wav_channels(&sample_path).unwrap_or(1);

    Ok(SfzRegion {
        buffer_id,
        num_channels,
        sample_path,
        key_range,
        vel_range,
        trigger,
        loop_mode,
        loop_start,
        loop_end,
        opcodes,
        group,
        off_by,
        seq_position,
        seq_length,
        buffer_frames,
        sample_rate,
    })
}

/// Detect the number of channels in a WAV file.
fn detect_wav_channels(path: &Path) -> Result<u32> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path)?;
    let mut header = [0u8; 44];
    file.read_exact(&mut header)?;

    // Check RIFF header
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        anyhow::bail!("Not a valid WAV file");
    }

    // Channels is at offset 22 (2 bytes, little-endian)
    let channels = u16::from_le_bytes([header[22], header[23]]) as u32;

    Ok(channels)
}

/// Read WAV file info (frames and sample rate).
fn read_wav_info(path: &Path) -> Result<(u32, f32)> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path)?;
    let mut header = [0u8; 44];
    file.read_exact(&mut header)?;

    // Check RIFF header
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        anyhow::bail!("Not a valid WAV file");
    }

    // Sample rate is at offset 24 (4 bytes, little-endian)
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]) as f32;

    // Bits per sample at offset 34 (2 bytes)
    let bits_per_sample = u16::from_le_bytes([header[34], header[35]]);

    // Channels at offset 22
    let channels = u16::from_le_bytes([header[22], header[23]]);

    // Data chunk size at offset 40 (4 bytes)
    let data_size = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);

    // Calculate frame count
    let bytes_per_sample = (bits_per_sample / 8) as u32;
    let bytes_per_frame = bytes_per_sample * channels as u32;
    let frame_count = data_size / bytes_per_frame;

    Ok((frame_count, sample_rate))
}

/// Parse all SFZ opcodes from a region.
fn parse_region_opcodes(
    sfz_region: &SfzSection,
    global_opcodes: &HashMap<String, String>,
) -> Result<SfzRegionOpcodes> {
    let mut opcodes = SfzRegionOpcodes::default();

    // Helper to get opcode value (region overrides global)
    let get_opcode = |name: &str| {
        sfz_region
            .get_opcode_str(name)
            .or_else(|| global_opcodes.get(name).map(|s| s.as_str()))
    };

    // Sound source & playback
    opcodes.offset = get_opcode("offset").and_then(|s| s.parse().ok());
    opcodes.offset_random = get_opcode("offset_random").and_then(|s| s.parse().ok());
    opcodes.pitch_keycenter = get_opcode("pitch_keycenter").and_then(|s| s.parse().ok());
    opcodes.pitch_keytrack = get_opcode("pitch_keytrack").and_then(|s| s.parse().ok());
    opcodes.tune = get_opcode("tune").and_then(|s| s.parse().ok());
    opcodes.transpose = get_opcode("transpose").and_then(|s| s.parse().ok());

    // Amplitude envelope
    opcodes.ampeg_attack = get_opcode("ampeg_attack").and_then(|s| s.parse().ok());
    opcodes.ampeg_hold = get_opcode("ampeg_hold").and_then(|s| s.parse().ok());
    opcodes.ampeg_decay = get_opcode("ampeg_decay").and_then(|s| s.parse().ok());
    opcodes.ampeg_sustain = get_opcode("ampeg_sustain").and_then(|s| s.parse().ok());
    opcodes.ampeg_release = get_opcode("ampeg_release").and_then(|s| s.parse().ok());
    opcodes.ampeg_vel2attack = get_opcode("ampeg_vel2attack").and_then(|s| s.parse().ok());
    opcodes.ampeg_vel2decay = get_opcode("ampeg_vel2decay").and_then(|s| s.parse().ok());
    opcodes.ampeg_vel2sustain = get_opcode("ampeg_vel2sustain").and_then(|s| s.parse().ok());
    opcodes.ampeg_vel2release = get_opcode("ampeg_vel2release").and_then(|s| s.parse().ok());

    // Filter
    opcodes.cutoff = get_opcode("cutoff").and_then(|s| s.parse().ok());
    opcodes.resonance = get_opcode("resonance").and_then(|s| s.parse().ok());
    opcodes.fil_type = get_opcode("fil_type").and_then(FilterType::from_str);
    opcodes.fil_keytrack = get_opcode("fil_keytrack").and_then(|s| s.parse().ok());
    opcodes.fil_keycenter = get_opcode("fil_keycenter").and_then(|s| s.parse().ok());
    opcodes.fil_veltrack = get_opcode("fil_veltrack").and_then(|s| s.parse().ok());

    // Filter envelope
    opcodes.fileg_attack = get_opcode("fileg_attack").and_then(|s| s.parse().ok());
    opcodes.fileg_hold = get_opcode("fileg_hold").and_then(|s| s.parse().ok());
    opcodes.fileg_decay = get_opcode("fileg_decay").and_then(|s| s.parse().ok());
    opcodes.fileg_sustain = get_opcode("fileg_sustain").and_then(|s| s.parse().ok());
    opcodes.fileg_release = get_opcode("fileg_release").and_then(|s| s.parse().ok());
    opcodes.fileg_depth = get_opcode("fileg_depth").and_then(|s| s.parse().ok());

    // Pitch envelope
    opcodes.pitcheg_attack = get_opcode("pitcheg_attack").and_then(|s| s.parse().ok());
    opcodes.pitcheg_hold = get_opcode("pitcheg_hold").and_then(|s| s.parse().ok());
    opcodes.pitcheg_decay = get_opcode("pitcheg_decay").and_then(|s| s.parse().ok());
    opcodes.pitcheg_sustain = get_opcode("pitcheg_sustain").and_then(|s| s.parse().ok());
    opcodes.pitcheg_release = get_opcode("pitcheg_release").and_then(|s| s.parse().ok());
    opcodes.pitcheg_depth = get_opcode("pitcheg_depth").and_then(|s| s.parse().ok());

    // Performance
    opcodes.volume = get_opcode("volume").and_then(|s| s.parse().ok());
    opcodes.amplitude = get_opcode("amplitude").and_then(|s| s.parse().ok());
    opcodes.pan = get_opcode("pan").and_then(|s| s.parse().ok());
    opcodes.width = get_opcode("width").and_then(|s| s.parse().ok());
    opcodes.position = get_opcode("position").and_then(|s| s.parse().ok());

    // LFO - Amplitude
    opcodes.amplfo_freq = get_opcode("amplfo_freq").and_then(|s| s.parse().ok());
    opcodes.amplfo_depth = get_opcode("amplfo_depth").and_then(|s| s.parse().ok());

    // LFO - Filter
    opcodes.fillfo_freq = get_opcode("fillfo_freq").and_then(|s| s.parse().ok());
    opcodes.fillfo_depth = get_opcode("fillfo_depth").and_then(|s| s.parse().ok());

    // LFO - Pitch
    opcodes.pitchlfo_freq = get_opcode("pitchlfo_freq").and_then(|s| s.parse().ok());
    opcodes.pitchlfo_depth = get_opcode("pitchlfo_depth").and_then(|s| s.parse().ok());

    Ok(opcodes)
}

// Helper functions

fn extract_opcodes_from_section(section: Option<&SfzSection>) -> HashMap<String, String> {
    section
        .map(|s| s.opcodes.clone())
        .unwrap_or_default()
}

fn extract_key_range(section: &SfzSection) -> Result<(u8, u8)> {
    use crate::parser::opcodes::RegionLogicOpcodes;

    // Try to get explicit key first
    if let Ok(key) = section.key() {
        return Ok((key as u8, key as u8));
    }

    // Otherwise get lokey/hikey
    let lokey = section.lokey().unwrap_or(0) as u8;
    let hikey = section.hikey().unwrap_or(127) as u8;

    Ok((lokey, hikey))
}

fn extract_vel_range(section: &SfzSection) -> (u8, u8) {
    use crate::parser::opcodes::RegionLogicOpcodes;

    let lovel = section.lovel().unwrap_or(0) as u8;
    let hivel = section.hivel().unwrap_or(127) as u8;

    (lovel, hivel)
}

fn extract_trigger_mode(section: &SfzSection) -> TriggerMode {
    use crate::parser::opcodes::RegionLogicOpcodes;

    section.trigger().unwrap_or(TriggerMode::Attack)
}

fn extract_loop_mode(section: &SfzSection) -> LoopMode {
    use crate::parser::opcodes::SamplePlaybackOpcodes;

    section.loop_mode().unwrap_or(LoopMode::NoLoop)
}

fn extract_opcode_u32(section: &SfzSection, name: &str) -> Option<u32> {
    section.get_opcode_str(name)?.parse().ok()
}

fn extract_opcode_i64(section: &SfzSection, name: &str) -> Option<i64> {
    section.get_opcode_str(name)?.parse().ok()
}
