//! Centralized OSC communication layer for scsynth.
//!
//! This module provides a single point for all OSC communication with SuperCollider,
//! enabling consistent score capture for offline rendering.

use crate::scsynth::{AddAction, BufNum, NodeId, Scsynth, Target};
use crate::score::ScoreWriter;
use crate::timing::{BeatTime, TransportClock};
use anyhow::Result;
use rosc::{OscMessage, OscPacket, OscType};
use std::path::PathBuf;
use std::time::Instant;

/// Convert OscTiming to seconds for score capture.
///
/// This is a free function to avoid borrow checker issues when
/// called while the score capture is mutably borrowed.
fn timing_to_seconds(timing: OscTiming, current_beat: f64, start_beat: f64, tempo: f64) -> f64 {
    match timing {
        OscTiming::Setup => 0.0,
        OscTiming::Now => {
            let relative_beat = current_beat - start_beat;
            crate::score::beats_to_seconds(relative_beat.max(0.0), tempo)
        }
        OscTiming::AtBeat(beat) => {
            let relative_beat = beat.to_float() - start_beat;
            crate::score::beats_to_seconds(relative_beat.max(0.0), tempo)
        }
    }
}

/// State for score capture mode.
pub struct ScoreCaptureState {
    /// The score writer accumulating events.
    pub writer: ScoreWriter,
    /// Output path for the score file.
    pub output_path: PathBuf,
    /// Beat at which capture started (for relative timing).
    pub start_beat: f64,
}

/// Message timing mode for OSC sends.
#[derive(Debug, Clone, Copy)]
pub enum OscTiming {
    /// Message happens at time 0 (setup: synthdefs, groups, buffers).
    Setup,
    /// Message happens at the current transport beat.
    Now,
    /// Message happens at a specific beat time.
    AtBeat(BeatTime),
}

/// Centralized OSC sender that handles all communication with scsynth.
///
/// All OSC messages flow through this component, enabling:
/// - Consistent score capture for offline rendering
/// - Centralized timing calculation
/// - Single point of control for all scsynth communication
pub struct OscSender {
    /// The underlying scsynth connection.
    sc: Scsynth,
    /// Score capture state (if recording is enabled).
    score_capture: Option<ScoreCaptureState>,
    /// Current tempo in BPM (for time calculations).
    tempo: f64,
}

impl OscSender {
    /// Create a new OscSender wrapping the given Scsynth connection.
    pub fn new(sc: Scsynth) -> Self {
        Self {
            sc,
            score_capture: None,
            tempo: 120.0,
        }
    }

    /// Get a reference to the underlying Scsynth.
    pub fn scsynth(&self) -> &Scsynth {
        &self.sc
    }

    /// Update the tempo used for beat-to-time calculations.
    pub fn set_tempo(&mut self, tempo: f64) {
        self.tempo = tempo;
    }

    /// Get the current tempo.
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Enable score capture to the given path.
    ///
    /// All subsequent OSC messages will be captured for offline rendering.
    pub fn enable_capture(&mut self, path: PathBuf) {
        log::info!(
            "[OSC_SENDER] Enabling score capture to {} (from beat 0)",
            path.display()
        );

        self.score_capture = Some(ScoreCaptureState {
            writer: ScoreWriter::new(),
            output_path: path,
            start_beat: 0.0,
        });
    }

    /// Disable score capture and write the score file.
    ///
    /// Returns the path where the score was written, or None if capture wasn't enabled.
    pub fn disable_capture(&mut self) -> Option<PathBuf> {
        let capture = self.score_capture.take()?;
        let path = capture.output_path.clone();
        let mut writer = capture.writer;

        log::info!(
            "[OSC_SENDER] Disabling score capture, writing {} events to {}",
            writer.event_count(),
            path.display()
        );

        if let Err(e) = writer.write_to_vibescore(&path) {
            log::error!("[OSC_SENDER] Failed to write score file: {}", e);
            return None;
        }

        Some(path)
    }

    /// Check if score capture is currently enabled.
    pub fn is_capturing(&self) -> bool {
        self.score_capture.is_some()
    }

    /// Get mutable access to the score writer (for adding samples).
    pub fn score_writer_mut(&mut self) -> Option<&mut ScoreWriter> {
        self.score_capture.as_mut().map(|c| &mut c.writer)
    }

    // ========================================================================
    // Core send methods
    // ========================================================================

    /// Send a raw OSC message with the specified timing.
    ///
    /// This is the core method through which all messages flow.
    pub fn send_msg(
        &mut self,
        timing: OscTiming,
        addr: &str,
        args: Vec<OscType>,
        current_beat: f64,
    ) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(time_seconds, addr, args.clone());
        }

        // Send to scsynth
        self.sc.osc.send_msg(addr, args)
    }

    /// Send a raw OSC packet with the specified timing.
    pub fn send_packet(
        &mut self,
        timing: OscTiming,
        packet: OscPacket,
        current_beat: f64,
    ) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_packet(time_seconds, packet.clone());
        }

        // Send to scsynth
        let encoded = rosc::encoder::encode(&packet)?;
        self.sc.osc.send_raw(&encoded)?;
        Ok(())
    }

    /// Send a timed bundle at a specific beat.
    ///
    /// This is used for sample-accurate event scheduling.
    pub fn send_bundle_at_beat(
        &mut self,
        beat_time: BeatTime,
        packets: Vec<OscPacket>,
        transport: &TransportClock,
        now: Instant,
    ) -> Result<()> {
        if packets.is_empty() {
            return Ok(());
        }

        // Convert beat time to OSC timestamp
        let (_, timestamp) = transport.beat_to_timestamp_and_instant(beat_time, now);

        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let relative_beat = beat_time.to_float() - capture.start_beat;
            let time_seconds = crate::score::beats_to_seconds(relative_beat, self.tempo);
            capture.writer.add_bundle(time_seconds, packets.clone());
        }

        // Send to scsynth
        self.sc.osc.send_bundle(Some(timestamp), packets)
    }

    // ========================================================================
    // High-level scsynth commands
    // ========================================================================

    /// Load a SynthDef from raw bytes.
    ///
    /// Synthdefs are always captured at time 0 (setup phase).
    pub fn d_recv(&mut self, bytes: Vec<u8>) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let packet = OscPacket::Message(OscMessage {
                addr: "/d_recv".to_string(),
                args: vec![OscType::Blob(bytes.clone())],
            });
            capture.writer.add_packet(0.0, packet);
            log::debug!("[OSC_SENDER] Captured synthdef at time 0");
        }

        // Send to scsynth
        self.sc.d_recv_bytes(bytes)
    }

    /// Create a new group node.
    ///
    /// Groups are captured at time 0 (setup phase).
    pub fn g_new(&mut self, node_id: NodeId, add_action: AddAction, target: Target) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            capture.writer.add_message(
                0.0,
                "/g_new",
                vec![
                    OscType::Int(node_id.as_i32()),
                    OscType::Int(add_action.into()),
                    OscType::Int(target.as_i32()),
                ],
            );
            log::debug!(
                "[OSC_SENDER] Captured group creation: node={} at time 0",
                node_id.as_i32()
            );
        }

        // Send to scsynth
        self.sc.g_new(node_id, add_action, target)
    }

    /// Create a new synth node.
    pub fn s_new(
        &mut self,
        timing: OscTiming,
        def: &str,
        node_id: NodeId,
        add_action: AddAction,
        target: Target,
        controls: &[(impl AsRef<str>, f32)],
        current_beat: f64,
    ) -> Result<()> {
        // Build args
        let mut args: Vec<OscType> = vec![
            OscType::String(def.to_string()),
            OscType::Int(node_id.as_i32()),
            OscType::Int(add_action.into()),
            OscType::Int(target.as_i32()),
        ];
        for (k, v) in controls {
            args.push(OscType::String(k.as_ref().to_string()));
            args.push(OscType::Float(*v));
        }

        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(time_seconds, "/s_new", args.clone());
            log::debug!(
                "[OSC_SENDER] Captured synth creation: def='{}' node={} at {:.3}s",
                def,
                node_id.as_i32(),
                time_seconds
            );
        }

        // Send to scsynth
        self.sc.s_new(def, node_id, add_action, target, controls)
    }

    /// Set control values on an existing node.
    pub fn n_set(
        &mut self,
        timing: OscTiming,
        node_id: NodeId,
        controls: &[(impl AsRef<str>, f32)],
        current_beat: f64,
    ) -> Result<()> {
        // Build args
        let mut args: Vec<OscType> = vec![OscType::Int(node_id.as_i32())];
        for (k, v) in controls {
            args.push(OscType::String(k.as_ref().to_string()));
            args.push(OscType::Float(*v));
        }

        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(time_seconds, "/n_set", args.clone());
        }

        // Send to scsynth
        self.sc.n_set(node_id, controls)
    }

    /// Free (stop and remove) a node.
    pub fn n_free(&mut self, timing: OscTiming, node_id: NodeId, current_beat: f64) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(
                time_seconds,
                "/n_free",
                vec![OscType::Int(node_id.as_i32())],
            );
        }

        // Send to scsynth
        self.sc.n_free(node_id)
    }

    /// Pause or resume a node.
    pub fn n_run(
        &mut self,
        timing: OscTiming,
        node_id: NodeId,
        run: bool,
        current_beat: f64,
    ) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(
                time_seconds,
                "/n_run",
                vec![
                    OscType::Int(node_id.as_i32()),
                    OscType::Int(if run { 1 } else { 0 }),
                ],
            );
        }

        // Send to scsynth
        self.sc.n_run(node_id, run)
    }

    /// Allocate a buffer and read an audio file into it.
    ///
    /// Also tracks the sample for inclusion in vibescore archives.
    pub fn b_alloc_read(
        &mut self,
        timing: OscTiming,
        bufnum: BufNum,
        path: &str,
        current_beat: f64,
    ) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);

            // Track the sample for archive inclusion
            capture.writer.add_sample(bufnum.as_i32(), path.to_string());

            // Add the b_allocRead message
            capture.writer.add_message(
                time_seconds,
                "/b_allocRead",
                vec![
                    OscType::Int(bufnum.as_i32()),
                    OscType::String(path.to_string()),
                    OscType::Int(0),  // start frame
                    OscType::Int(-1), // num frames (-1 = all)
                ],
            );

            log::debug!(
                "[OSC_SENDER] Captured buffer load: buf={} path='{}' at {:.3}s",
                bufnum.as_i32(),
                path,
                time_seconds
            );
        }

        // Send to scsynth
        self.sc.b_alloc_read(bufnum, path)
    }

    /// Free a buffer.
    pub fn b_free(&mut self, timing: OscTiming, bufnum: BufNum, current_beat: f64) -> Result<()> {
        // Capture to score if enabled
        if let Some(ref mut capture) = self.score_capture {
            let time_seconds = timing_to_seconds(timing, current_beat, capture.start_beat, self.tempo);
            capture.writer.add_message(
                time_seconds,
                "/b_free",
                vec![OscType::Int(bufnum.as_i32())],
            );
        }

        // Send to scsynth
        self.sc.osc.send_msg("/b_free", vec![OscType::Int(bufnum.as_i32())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPO_120: f64 = 120.0;
    const TEMPO_140: f64 = 140.0;

    #[test]
    fn test_timing_to_seconds_setup() {
        // Setup timing should always return 0
        assert_eq!(timing_to_seconds(OscTiming::Setup, 10.0, 0.0, TEMPO_120), 0.0);
        assert_eq!(timing_to_seconds(OscTiming::Setup, 0.0, 5.0, TEMPO_120), 0.0);
    }

    #[test]
    fn test_timing_to_seconds_now() {
        // At beat 4, with start_beat 0, at 120 BPM should be 2 seconds
        let time = timing_to_seconds(OscTiming::Now, 4.0, 0.0, TEMPO_120);
        assert!((time - 2.0).abs() < 0.001);

        // At beat 8, with start_beat 4, should be 2 seconds (4 beats = 2s at 120bpm)
        let time = timing_to_seconds(OscTiming::Now, 8.0, 4.0, TEMPO_120);
        assert!((time - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_timing_to_seconds_at_beat() {
        // Beat 4 at 120 BPM = 2 seconds
        let time = timing_to_seconds(OscTiming::AtBeat(BeatTime::from_float(4.0)), 0.0, 0.0, TEMPO_120);
        assert!((time - 2.0).abs() < 0.001);

        // Beat 8 with start_beat 4 = 4 beats = 2 seconds
        let time = timing_to_seconds(OscTiming::AtBeat(BeatTime::from_float(8.0)), 0.0, 4.0, TEMPO_120);
        assert!((time - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_timing_to_seconds_negative_clamped() {
        // If current beat is before start beat, should clamp to 0
        let time = timing_to_seconds(OscTiming::Now, 2.0, 4.0, TEMPO_120);
        assert_eq!(time, 0.0);
    }

    #[test]
    fn test_timing_with_different_tempos() {
        // At 140 BPM, 4 beats = 4 * 60 / 140 = 1.714... seconds
        let time = timing_to_seconds(OscTiming::Now, 4.0, 0.0, TEMPO_140);
        let expected = 4.0 * 60.0 / 140.0;
        assert!((time - expected).abs() < 0.001);
    }

    #[test]
    fn test_timing_at_beat_ignores_current_beat() {
        // AtBeat should use the specified beat, not current_beat
        let time1 = timing_to_seconds(OscTiming::AtBeat(BeatTime::from_float(8.0)), 0.0, 0.0, TEMPO_120);
        let time2 = timing_to_seconds(OscTiming::AtBeat(BeatTime::from_float(8.0)), 100.0, 0.0, TEMPO_120);
        assert!((time1 - time2).abs() < 0.001);
        assert!((time1 - 4.0).abs() < 0.001); // 8 beats at 120 BPM = 4 seconds
    }

    // Test the capture state struct directly
    #[test]
    fn test_score_capture_state() {
        let state = ScoreCaptureState {
            writer: ScoreWriter::new(),
            output_path: PathBuf::from("/tmp/test.vibescore"),
            start_beat: 0.0,
        };
        assert_eq!(state.start_beat, 0.0);
        assert_eq!(state.output_path, PathBuf::from("/tmp/test.vibescore"));
    }
}
