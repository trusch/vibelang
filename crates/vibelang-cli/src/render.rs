//! Offline rendering implementation for VibeLang.
//!
//! This module provides functionality to render a `.vibescore` file to an audio file
//! using SuperCollider's non-realtime (NRT) mode.
//!
//! The `.vibescore` format is a tar archive containing:
//! - `score.osc`: OSC events for playback
//! - `synthdefs/*.scsyndef`: Individual synthdef files
//! - `samples/*.wav`: Sample files referenced by b_allocRead

use crate::RenderArgs;
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use std::process::Command;
use tar::Archive;

/// Render a score file to an audio file (standalone command with logger init).
pub fn render(args: RenderArgs) -> Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    render_score(args)
}

/// Render a score file to an audio file (callable from other code).
pub fn render_score(args: RenderArgs) -> Result<()> {
    log::info!("VibeLang Render");
    log::info!("==============");
    log::info!("Input:  {}", args.score_file.display());
    log::info!("Output: {}", args.output.display());

    // Validate input file
    if !args.score_file.exists() {
        anyhow::bail!("Score file not found: {}", args.score_file.display());
    }

    // Determine output format from extension or flag
    let output_format = args.format.clone().unwrap_or_else(|| {
        args.output
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("wav")
            .to_lowercase()
    });

    // Create temporary directory for intermediate files if needed
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let wav_path = if output_format != "wav" {
        temp_dir.path().join("output.wav")
    } else {
        args.output.clone()
    };

    // Validate input file extension
    let is_vibescore = args.score_file.extension()
        .map(|e| e == "vibescore")
        .unwrap_or(false);

    if !is_vibescore {
        anyhow::bail!(
            "Invalid input file: expected .vibescore format\n\
            Record a .vibescore file using: vibe run <file.vibe> --record output.vibescore"
        );
    }

    log::info!("Score file size: {} bytes", fs::metadata(&args.score_file)?.len());

    // Create temp directory for extraction
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;

    // Extract vibescore archive
    log::info!("\nExtracting vibescore archive...");
    let (score_path, synthdef_dir, samples_dir) = extract_vibescore(&args.score_file, temp_dir.path())?;

    // Render with scsynth
    log::info!("\n[1/2] Rendering audio with scsynth...");
    let bit_depth_str = match args.bit_depth {
        16 => "int16",
        24 => "int24",
        32 => "float",
        _ => "int24",
    };
    run_scsynth_nrt(&score_path, &synthdef_dir, &samples_dir, &wav_path, args.sample_rate, bit_depth_str)?;

    // Convert to final format if needed
    if output_format != "wav" {
        log::info!("\n[2/2] Converting to {}...", output_format);
        convert_with_ffmpeg(&wav_path, &args.output, &output_format)?;
    } else {
        log::info!("\n[2/2] Output format is WAV, no conversion needed.");
    }

    log::info!("\nRender complete: {}", args.output.display());
    log::info!("File size: {} bytes", fs::metadata(&args.output)?.len());

    Ok(())
}

/// Run scsynth in NRT mode to render the score.
fn run_scsynth_nrt(
    score_path: &Path,
    synthdef_dir: &Path,
    samples_dir: &Path,
    output_path: &Path,
    sample_rate: u32,
    bit_depth: &str,
) -> Result<()> {
    // Find scsynth
    let scsynth = which::which("scsynth").context(
        "scsynth not found in PATH. Please install SuperCollider."
    )?;

    log::info!("       Using scsynth: {}", scsynth.display());
    log::info!("       Sample rate: {}", sample_rate);
    log::info!("       Bit depth: {}", bit_depth);
    log::info!("       Score: {}", score_path.display());
    log::info!("       Synthdefs: {}", synthdef_dir.display());
    log::info!("       Samples: {}", samples_dir.display());

    // Count synthdefs
    let synthdef_count = fs::read_dir(synthdef_dir)
        .map(|entries| entries.filter(|e| {
            if let Ok(entry) = e {
                entry.path().extension()
                    .map(|ext| ext == "scsyndef")
                    .unwrap_or(false)
            } else {
                false
            }
        }).count())
        .unwrap_or(0);
    log::info!("       Found {} synthdefs", synthdef_count);

    // Count samples
    let sample_count = fs::read_dir(samples_dir)
        .map(|entries| entries.count())
        .unwrap_or(0);
    log::info!("       Found {} samples", sample_count);

    // Create filtered score with /d_loadDir command and resolved sample paths
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let filtered_score_path = temp_dir.path().join("score_with_loaddir.osc");
    add_loaddir_to_score(score_path, synthdef_dir, samples_dir, &filtered_score_path)?;

    // scsynth -N <score.osc> _ <output.wav> <sample_rate> WAV <bit_depth> -o 2
    let output = Command::new(&scsynth)
        .arg("-N")
        .arg(&filtered_score_path)
        .arg("_")  // No input file
        .arg(output_path)
        .arg(sample_rate.to_string())
        .arg("WAV")
        .arg(bit_depth)
        .arg("-o")
        .arg("2")  // Stereo output
        .output()
        .context("Failed to run scsynth")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        log::error!("scsynth stdout: {}", stdout);
        log::error!("scsynth stderr: {}", stderr);
        anyhow::bail!("scsynth failed with exit code: {:?}", output.status.code());
    }

    log::info!("       scsynth completed successfully");
    Ok(())
}

/// Create an OSC bundle with /d_loadDir command at time 0.
fn create_loaddir_bundle(synthdef_dir: &Path) -> Result<Vec<u8>> {
    use rosc::{OscBundle, OscMessage, OscPacket, OscTime, OscType, encoder};

    let dir_path = synthdef_dir.to_string_lossy().to_string();

    let message = OscMessage {
        addr: "/d_loadDir".to_string(),
        args: vec![OscType::String(dir_path)],
    };

    let bundle = OscPacket::Bundle(OscBundle {
        timetag: OscTime::from((1u32, 0u32)), // Time 0 (we use 1 as base)
        content: vec![OscPacket::Message(message)],
    });

    encoder::encode(&bundle)
        .map_err(|e| anyhow::anyhow!("Failed to encode /d_loadDir bundle: {}", e))
}

/// Extract a bundled .vibescore archive to a temporary directory.
/// Returns (score_path, synthdef_dir, samples_dir).
fn extract_vibescore(archive_path: &Path, dest_dir: &Path) -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf)> {
    let file = File::open(archive_path).context("Failed to open vibescore archive")?;
    let mut archive = Archive::new(file);

    let synthdef_dir = dest_dir.join("synthdefs");
    let samples_dir = dest_dir.join("samples");
    fs::create_dir_all(&synthdef_dir)?;
    fs::create_dir_all(&samples_dir)?;

    let mut score_path = None;
    let mut synthdef_count = 0;
    let mut sample_count = 0;

    for entry in archive.entries().context("Failed to read archive entries")? {
        let mut entry = entry.context("Failed to read archive entry")?;
        let path = entry.path().context("Failed to get entry path")?;
        let path_str = path.to_string_lossy();

        if path_str == "score.osc" {
            // Extract score file
            let dest_path = dest_dir.join("score.osc");
            let mut dest_file = File::create(&dest_path)?;
            std::io::copy(&mut entry, &mut dest_file)?;
            score_path = Some(dest_path);
            log::info!("       Extracted score.osc");
        } else if path_str.starts_with("synthdefs/") && path_str.ends_with(".scsyndef") {
            // Extract synthdef file
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !filename.is_empty() {
                let dest_path = synthdef_dir.join(&filename);
                let mut dest_file = File::create(&dest_path)?;
                std::io::copy(&mut entry, &mut dest_file)?;
                log::debug!("       Extracted synthdef: {}", filename);
                synthdef_count += 1;
            }
        } else if path_str.starts_with("samples/") {
            // Extract sample file
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !filename.is_empty() {
                let dest_path = samples_dir.join(&filename);
                let mut dest_file = File::create(&dest_path)?;
                std::io::copy(&mut entry, &mut dest_file)?;
                log::debug!("       Extracted sample: {}", filename);
                sample_count += 1;
            }
        }
    }

    let score_path = score_path.context("No score.osc found in vibescore archive")?;
    log::info!("       Extracted {} synthdefs", synthdef_count);
    log::info!("       Extracted {} samples", sample_count);

    Ok((score_path, synthdef_dir, samples_dir))
}

/// Add /d_loadDir command to the beginning of a score file and resolve sample paths.
/// This is used for clean score files (from .vibescore) that don't have embedded synthdefs.
fn add_loaddir_to_score(
    input_path: &Path,
    synthdef_dir: &Path,
    samples_dir: &Path,
    output_path: &Path,
) -> Result<()> {
    use std::io::Write;

    let input_file = File::open(input_path).context("Failed to open score file")?;
    let mut reader = BufReader::new(input_file);

    let output_file = File::create(output_path)?;
    let mut writer = std::io::BufWriter::new(output_file);

    // Write /d_loadDir bundle first
    let loaddir_bundle = create_loaddir_bundle(synthdef_dir)?;
    writer.write_all(&(loaddir_bundle.len() as i32).to_be_bytes())?;
    writer.write_all(&loaddir_bundle)?;
    log::debug!("       Added /d_loadDir for {}", synthdef_dir.display());

    let samples_dir_str = samples_dir.to_string_lossy().to_string();

    // Copy the rest of the score, resolving sample paths and skipping empty bundles
    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }

        let len = i32::from_be_bytes(len_buf);

        // Skip zero length terminator (for backward compatibility with old score files)
        if len == 0 {
            break;
        }

        if len < 0 {
            anyhow::bail!("Invalid score file: negative length {}", len);
        }

        let mut bundle_data = vec![0u8; len as usize];
        reader.read_exact(&mut bundle_data)?;

        // Decode, process, and re-encode the bundle
        match rosc::decoder::decode_udp(&bundle_data) {
            Ok((_, packet)) => {
                // Check if empty bundle
                let is_empty = if let rosc::OscPacket::Bundle(ref bundle) = packet {
                    bundle.content.is_empty()
                } else {
                    false
                };

                if !is_empty {
                    // Resolve {SAMPLES_DIR} placeholders
                    let processed_packet = resolve_sample_paths(&packet, &samples_dir_str);

                    // Re-encode the packet
                    let encoded = rosc::encoder::encode(&processed_packet)
                        .map_err(|e| anyhow::anyhow!("Failed to encode OSC packet: {}", e))?;

                    writer.write_all(&(encoded.len() as i32).to_be_bytes())?;
                    writer.write_all(&encoded)?;
                }
            }
            Err(_) => {
                // If we can't decode, just copy as-is
                writer.write_all(&len.to_be_bytes())?;
                writer.write_all(&bundle_data)?;
            }
        }
    }

    // scsynth NRT mode expects EOF to signal end of file, not a zero-length marker
    writer.flush()?;
    Ok(())
}

/// Resolve {SAMPLES_DIR} placeholders in b_allocRead messages.
fn resolve_sample_paths(packet: &rosc::OscPacket, samples_dir: &str) -> rosc::OscPacket {
    match packet {
        rosc::OscPacket::Message(msg) => {
            if msg.addr == "/b_allocRead" && msg.args.len() >= 2 {
                let mut new_args = msg.args.clone();
                // Second argument is the file path
                if let rosc::OscType::String(ref path) = msg.args[1] {
                    if path.contains("{SAMPLES_DIR}") {
                        let resolved = path.replace("{SAMPLES_DIR}", samples_dir);
                        new_args[1] = rosc::OscType::String(resolved);
                        log::debug!("       Resolved sample path: {} -> {}", path,
                            if let rosc::OscType::String(ref s) = new_args[1] { s } else { "?" });
                    }
                }
                rosc::OscPacket::Message(rosc::OscMessage {
                    addr: msg.addr.clone(),
                    args: new_args,
                })
            } else {
                rosc::OscPacket::Message(msg.clone())
            }
        }
        rosc::OscPacket::Bundle(bundle) => {
            rosc::OscPacket::Bundle(rosc::OscBundle {
                timetag: bundle.timetag,
                content: bundle.content.iter().map(|p| resolve_sample_paths(p, samples_dir)).collect(),
            })
        }
    }
}

/// Convert audio file using ffmpeg.
fn convert_with_ffmpeg(input: &Path, output: &Path, format: &str) -> Result<()> {
    // Check if ffmpeg is available
    let ffmpeg = which::which("ffmpeg").context(
        "ffmpeg not found in PATH. Please install ffmpeg for format conversion."
    )?;

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")  // Overwrite output
       .arg("-i").arg(input);

    // Format-specific options
    match format {
        "mp3" => {
            cmd.arg("-codec:a").arg("libmp3lame")
               .arg("-b:a").arg("320k");
        }
        "flac" => {
            cmd.arg("-codec:a").arg("flac");
        }
        "ogg" => {
            cmd.arg("-codec:a").arg("libvorbis")
               .arg("-q:a").arg("8");
        }
        "aac" | "m4a" => {
            cmd.arg("-codec:a").arg("aac")
               .arg("-b:a").arg("256k");
        }
        _ => {
            log::warn!("Unknown format '{}', using default settings", format);
        }
    }

    cmd.arg(output);

    let output_result = cmd.output().context("Failed to run ffmpeg")?;

    if !output_result.status.success() {
        let stderr = String::from_utf8_lossy(&output_result.stderr);
        anyhow::bail!("ffmpeg failed: {}", stderr);
    }

    log::info!("       Converted to {}", format);
    Ok(())
}
