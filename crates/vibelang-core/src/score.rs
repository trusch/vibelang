//! SuperCollider score file writer for NRT (non-realtime) rendering.
//!
//! This module provides functionality to capture OSC events and write them
//! to a binary score file that can be processed by scsynth in non-realtime mode.
//!
//! The score file format is:
//! - For each event: 4-byte big-endian length prefix + encoded OSC bundle
//! - Final marker: 4-byte zero length

use rosc::{encoder, OscBundle, OscMessage, OscPacket, OscTime, OscType};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// A captured event with absolute time in seconds.
#[derive(Debug, Clone)]
pub struct ScoredEvent {
    /// Time in seconds from the start of the score.
    pub time_seconds: f64,
    /// The OSC packet (message or bundle).
    pub packet: OscPacket,
}

/// Score file writer that accumulates events and writes binary OSC format.
///
/// Events are captured during playback and can be written to a file
/// for offline rendering with scsynth -N.
pub struct ScoreWriter {
    /// Accumulated events.
    pub events: Vec<ScoredEvent>,
    /// Sample files to include in the archive (buffer_id -> file_path).
    pub samples: std::collections::HashMap<i32, String>,
}

impl Default for ScoreWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ScoreWriter {
    /// Create a new empty score writer.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            samples: std::collections::HashMap::new(),
        }
    }

    /// Add a sample file to be included in the archive.
    pub fn add_sample(&mut self, buffer_id: i32, path: String) {
        self.samples.insert(buffer_id, path);
    }

    /// Add an OSC message at the given time.
    pub fn add_message(&mut self, time_seconds: f64, addr: &str, args: Vec<OscType>) {
        let msg = OscMessage {
            addr: addr.to_string(),
            args,
        };
        self.events.push(ScoredEvent {
            time_seconds,
            packet: OscPacket::Message(msg),
        });
    }

    /// Add a raw OSC packet at the given time.
    pub fn add_packet(&mut self, time_seconds: f64, packet: OscPacket) {
        self.events.push(ScoredEvent {
            time_seconds,
            packet,
        });
    }

    /// Add a bundle of OSC messages at the given time.
    pub fn add_bundle(&mut self, time_seconds: f64, packets: Vec<OscPacket>) {
        let timetag = seconds_to_osc_time(time_seconds);
        let bundle = OscPacket::Bundle(OscBundle {
            timetag,
            content: packets,
        });
        self.events.push(ScoredEvent {
            time_seconds,
            packet: bundle,
        });
    }

    /// Get the total duration of the score in seconds.
    pub fn duration(&self) -> f64 {
        self.events
            .iter()
            .map(|e| e.time_seconds)
            .fold(0.0, f64::max)
    }

    /// Get the number of events in the score.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Sort events by time and write to a binary score file.
    ///
    /// The file format is compatible with SuperCollider's NRT mode:
    /// - Each bundle is prefixed with a 4-byte big-endian length
    /// - Events are sorted by time
    /// - A final zero-length marker indicates end of score
    pub fn write_to_file(&mut self, path: &Path) -> std::io::Result<()> {
        // Sort events by time
        self.events
            .sort_by(|a, b| a.time_seconds.partial_cmp(&b.time_seconds).unwrap());

        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        for event in &self.events {
            // Wrap non-bundle packets in a bundle with appropriate timetag
            let packet_to_write = match &event.packet {
                OscPacket::Bundle(_) => event.packet.clone(),
                OscPacket::Message(msg) => {
                    // Wrap message in a bundle
                    OscPacket::Bundle(OscBundle {
                        timetag: seconds_to_osc_time(event.time_seconds),
                        content: vec![OscPacket::Message(msg.clone())],
                    })
                }
            };

            let encoded = encoder::encode(&packet_to_write)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            // Write 4-byte big-endian length prefix
            let len = encoded.len() as i32;
            writer.write_all(&len.to_be_bytes())?;

            // Write the encoded OSC data
            writer.write_all(&encoded)?;
        }

        // scsynth NRT mode expects EOF to signal end of file, not a zero-length marker
        writer.flush()
    }

    /// Clear all events from the score.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Write to a bundled .vibescore format (tar archive).
    ///
    /// The archive contains:
    /// - `score.osc`: The OSC events without synthdef /d_recv commands
    /// - `synthdefs/*.scsyndef`: Individual synthdef files
    /// - `samples/*.wav`: Sample files referenced by b_allocRead
    ///
    /// This format is cleaner than embedding synthdefs in the OSC file
    /// and avoids the 8192 byte message limit in scsynth NRT mode.
    pub fn write_to_vibescore(&mut self, path: &Path) -> std::io::Result<()> {
        use std::collections::HashMap;
        use tar::{Builder, Header};

        // Sort events by time
        self.events
            .sort_by(|a, b| a.time_seconds.partial_cmp(&b.time_seconds).unwrap());

        // Build a mapping from original sample paths to archive paths
        let mut sample_path_map: HashMap<String, String> = HashMap::new();
        for (buffer_id, original_path) in &self.samples {
            let fallback_name = format!("buffer_{}.wav", buffer_id);
            let filename = Path::new(original_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&fallback_name);
            let archive_path = format!("samples/{}", filename);
            sample_path_map.insert(original_path.clone(), archive_path);
        }

        // Separate synthdef events from other events and rewrite b_allocRead paths
        let mut synthdefs: HashMap<String, Vec<u8>> = HashMap::new();
        let mut processed_events: Vec<(f64, OscPacket)> = Vec::new();

        for event in &self.events {
            let is_synthdef = if let OscPacket::Message(msg) = &event.packet {
                if msg.addr == "/d_recv" {
                    // Extract synthdef name and bytes from args
                    for arg in &msg.args {
                        if let OscType::Blob(blob) = arg {
                            if let Some(name) = extract_synthdef_name(blob) {
                                synthdefs.insert(name, blob.clone());
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            } else if let OscPacket::Bundle(bundle) = &event.packet {
                // Check if bundle contains /d_recv
                let has_synthdef = bundle.content.iter().any(|p| {
                    if let OscPacket::Message(msg) = p {
                        if msg.addr == "/d_recv" {
                            for arg in &msg.args {
                                if let OscType::Blob(blob) = arg {
                                    if let Some(name) = extract_synthdef_name(blob) {
                                        synthdefs.insert(name, blob.clone());
                                    }
                                }
                            }
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                has_synthdef
            } else {
                false
            };

            if !is_synthdef {
                // Rewrite b_allocRead paths
                let packet = rewrite_sample_paths(&event.packet, &sample_path_map);
                processed_events.push((event.time_seconds, packet));
            }
        }

        // Create tar archive
        let file = File::create(path)?;
        let mut archive = Builder::new(file);

        // Write score.osc to the archive (without synthdef events)
        let mut score_data = Vec::new();
        for (time_seconds, packet) in &processed_events {
            let packet_to_write = match packet {
                OscPacket::Bundle(_) => packet.clone(),
                OscPacket::Message(msg) => {
                    OscPacket::Bundle(OscBundle {
                        timetag: seconds_to_osc_time(*time_seconds),
                        content: vec![OscPacket::Message(msg.clone())],
                    })
                }
            };

            let encoded = encoder::encode(&packet_to_write)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            // Write 4-byte big-endian length prefix
            let len = encoded.len() as i32;
            score_data.extend_from_slice(&len.to_be_bytes());
            score_data.extend_from_slice(&encoded);
        }
        // scsynth NRT mode expects EOF to signal end of file, not a zero-length marker

        let mut header = Header::new_gnu();
        header.set_path("score.osc")?;
        header.set_size(score_data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append(&header, score_data.as_slice())?;

        // Write synthdefs to synthdefs/ directory
        for (name, bytes) in &synthdefs {
            let mut header = Header::new_gnu();
            header.set_path(format!("synthdefs/{}.scsyndef", name))?;
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive.append(&header, bytes.as_slice())?;
        }

        // Write samples to samples/ directory
        for original_path in self.samples.values() {
            if let Some(archive_path) = sample_path_map.get(original_path) {
                // Read the sample file
                match std::fs::read(original_path) {
                    Ok(sample_data) => {
                        let mut header = Header::new_gnu();
                        header.set_path(archive_path)?;
                        header.set_size(sample_data.len() as u64);
                        header.set_mode(0o644);
                        header.set_cksum();
                        archive.append(&header, sample_data.as_slice())?;
                        log::debug!("[SCORE] Added sample: {} -> {}", original_path, archive_path);
                    }
                    Err(e) => {
                        log::warn!("[SCORE] Failed to read sample '{}': {}", original_path, e);
                    }
                }
            }
        }

        archive.finish()?;

        log::info!(
            "[SCORE] Wrote vibescore archive with {} synthdefs, {} samples, and {} events",
            synthdefs.len(),
            self.samples.len(),
            processed_events.len()
        );

        Ok(())
    }
}

/// Rewrite sample paths in b_allocRead messages to use archive-relative paths.
fn rewrite_sample_paths(packet: &OscPacket, path_map: &std::collections::HashMap<String, String>) -> OscPacket {
    match packet {
        OscPacket::Message(msg) => {
            if msg.addr == "/b_allocRead" && msg.args.len() >= 2 {
                let mut new_args = msg.args.clone();
                // Second argument is the file path
                if let OscType::String(ref path) = msg.args[1] {
                    if let Some(new_path) = path_map.get(path) {
                        // Use a placeholder that will be replaced with actual path during render
                        new_args[1] = OscType::String(format!("{{SAMPLES_DIR}}/{}", new_path.strip_prefix("samples/").unwrap_or(new_path)));
                    }
                }
                OscPacket::Message(OscMessage {
                    addr: msg.addr.clone(),
                    args: new_args,
                })
            } else {
                OscPacket::Message(msg.clone())
            }
        }
        OscPacket::Bundle(bundle) => {
            OscPacket::Bundle(OscBundle {
                timetag: bundle.timetag,
                content: bundle.content.iter().map(|p| rewrite_sample_paths(p, path_map)).collect(),
            })
        }
    }
}

/// Extract synthdef name from raw synthdef bytes.
///
/// SuperCollider synthdef format:
/// - 4 bytes: "SCgf" magic
/// - 4 bytes: file version
/// - 2 bytes: number of synthdefs
/// - pstring: 1 byte length + name bytes
pub fn extract_synthdef_name(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 12 {
        return None;
    }
    if &bytes[0..4] != b"SCgf" {
        return None;
    }
    let name_len = bytes[10] as usize;
    if bytes.len() < 11 + name_len {
        return None;
    }
    String::from_utf8(bytes[11..11 + name_len].to_vec()).ok()
}

/// Convert seconds to OSC NTP timetag.
///
/// OSC timetags use NTP format: seconds since Jan 1, 1900.
/// For score files, we use relative times from the start of the score.
pub fn seconds_to_osc_time(seconds: f64) -> OscTime {
    // Use 1 as the base to avoid the "immediately" special case (0,1)
    let secs = 1 + seconds.floor() as u32;
    let frac = ((seconds - seconds.floor()) * (u32::MAX as f64)) as u32;
    OscTime::from((secs, frac))
}

/// Convert beats to seconds given a tempo in BPM.
pub fn beats_to_seconds(beats: f64, bpm: f64) -> f64 {
    beats * 60.0 / bpm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_writer_new() {
        let score = ScoreWriter::new();
        assert_eq!(score.event_count(), 0);
        assert_eq!(score.duration(), 0.0);
    }

    #[test]
    fn test_add_message() {
        let mut score = ScoreWriter::new();
        score.add_message(1.0, "/test", vec![OscType::Int(42)]);
        assert_eq!(score.event_count(), 1);
        assert_eq!(score.duration(), 1.0);
    }

    #[test]
    fn test_add_bundle() {
        let mut score = ScoreWriter::new();
        let packets = vec![OscPacket::Message(OscMessage {
            addr: "/s_new".to_string(),
            args: vec![OscType::String("synth".to_string())],
        })];
        score.add_bundle(2.5, packets);
        assert_eq!(score.event_count(), 1);
        assert_eq!(score.duration(), 2.5);
    }

    #[test]
    fn test_seconds_to_osc_time() {
        let time = seconds_to_osc_time(0.0);
        assert_eq!(time.seconds, 1);
        assert_eq!(time.fractional, 0);

        let time = seconds_to_osc_time(1.5);
        assert_eq!(time.seconds, 2);
        // Fractional should be around half of u32::MAX
        assert!(time.fractional > u32::MAX / 4);
        assert!(time.fractional < (u32::MAX / 4) * 3);
    }

    #[test]
    fn test_beats_to_seconds() {
        assert!((beats_to_seconds(4.0, 120.0) - 2.0).abs() < 0.001);
        assert!((beats_to_seconds(1.0, 60.0) - 1.0).abs() < 0.001);
    }
}
