//! MIDI looper data types.
//!
//! Defines the state machine and runtime state for a single looper instance.

use crate::midi::recording::MidiRecording;
use crate::reload::{LooperConfig, LooperKey};
use crate::traits::PatternConfig;
use crate::types::ids::{MidiDeviceId, PatternId, VoiceId};
use crate::types::time::Beat;
use crate::validation::Validate;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Looper state machine states.
#[derive(Debug, Clone)]
pub enum LooperPhase {
    /// Waiting for first note-on.
    Idle,
    /// Recording in progress. Tracks last note-off for silence detection.
    Capturing {
        last_note_off_beat: Option<Beat>, // None = no note-off yet
    },
    /// Playback active. Holds the pattern id of the currently looping pattern.
    Playing {
        pattern_id: PatternId,
        loop_length_beats: f64, // length of the loop in beats
    },
}

/// Runtime state for a single looper instance.
#[derive(Debug)]
pub struct LooperInstance {
    pub config: LooperConfig,
    pub phase: LooperPhase,
    pub recording: MidiRecording, // Active or last completed recording
    pub loop_count: u64,          // Increments on each new capture (for unique pattern names)
}

impl LooperInstance {
    pub fn new(config: LooperConfig) -> Self {
        let device_id = config.device_id;
        Self {
            config,
            phase: LooperPhase::Idle,
            recording: MidiRecording::new(device_id, Beat::ZERO),
            loop_count: 0,
        }
    }
}

// =============================================================================
// LooperAction
// =============================================================================

/// Actions returned by LooperManager methods — the caller executes these.
#[derive(Debug, Clone)]
pub enum LooperAction {
    /// Pass a note-on through to the voice (during capture).
    NoteOn {
        voice_id: VoiceId,
        note: u8,
        velocity: u8,
    },
    /// Pass a note-off through to the voice.
    NoteOff { voice_id: VoiceId, note: u8 },
    /// Stop a looping pattern.
    StopPattern { pattern_id: PatternId },
    /// Start a new looping pattern from a completed recording.
    StartPattern {
        config: PatternConfig,
        pattern_id: PatternId,
    },
}

// =============================================================================
// LooperManager
// =============================================================================

/// Manages all active looper instances.
pub struct LooperManager {
    instances: HashMap<LooperKey, LooperInstance>,
}

impl Default for LooperManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LooperManager {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    /// Returns true if any looper instance exists for this device.
    pub fn has_device(&self, device_id: MidiDeviceId) -> bool {
        self.instances.keys().any(|key| key.device_id == device_id)
    }

    /// Returns true if a MIDI event would be consumed by a looper.
    pub fn has_route_for_event(&self, device_id: MidiDeviceId, channel: u8) -> bool {
        self.key_for_event(device_id, channel).is_some()
    }

    /// Reconcile instances against the new set of configs.
    ///
    /// - New `(device_id, channel)` keys → insert a fresh LooperInstance.
    /// - Removed keys → remove (returning StopPattern if Playing).
    /// - Changed configs for an existing key → stop any active pattern and reset.
    /// - Unchanged configs → preserve phase/recording/pattern ownership.
    pub fn reconcile(&mut self, configs: &[LooperConfig]) -> Vec<LooperAction> {
        let mut actions = Vec::new();

        // Build set of incoming looper keys for fast lookup.
        let incoming: HashMap<LooperKey, &LooperConfig> =
            configs.iter().map(|c| (c.key(), c)).collect();

        // Remove instances no longer in configs.
        self.instances.retain(|key, instance| {
            if incoming.contains_key(key) {
                true
            } else {
                // If it was playing, stop the pattern.
                if let LooperPhase::Playing { pattern_id, .. } = instance.phase {
                    actions.push(LooperAction::StopPattern { pattern_id });
                }
                false
            }
        });

        // Add new instances or update configs for existing ones.
        for config in configs {
            self.instances
                .entry(config.key())
                .and_modify(|inst| {
                    if inst.config != *config {
                        if let LooperPhase::Playing { pattern_id, .. } = inst.phase {
                            actions.push(LooperAction::StopPattern { pattern_id });
                        }
                        *inst = LooperInstance::new(config.clone());
                    }
                })
                .or_insert_with(|| LooperInstance::new(config.clone()));
        }

        actions
    }

    fn key_for_event(&self, device_id: MidiDeviceId, channel: u8) -> Option<LooperKey> {
        let channel_key = LooperKey::new(device_id, Some(channel));
        if self.instances.contains_key(&channel_key) {
            return Some(channel_key);
        }

        let all_channels_key = LooperKey::new(device_id, None);
        self.instances
            .contains_key(&all_channels_key)
            .then_some(all_channels_key)
    }

    /// Handle a note-on event for a device.
    pub fn handle_note_on(
        &mut self,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        velocity: u8,
        current_beat: f64,
        time_sig_numerator: u8,
    ) -> Vec<LooperAction> {
        let Some(key) = self.key_for_event(device_id, channel) else {
            return Vec::new();
        };
        let Some(inst) = self.instances.get_mut(&key) else {
            return Vec::new();
        };

        let beat = Beat::from_f64(current_beat);
        let voice_id = inst.config.voice_id;
        let mut actions = Vec::new();

        match inst.phase.clone() {
            LooperPhase::Idle => {
                // Start a fresh recording.
                let mut rec =
                    MidiRecording::new(device_id, bar_aligned_start(beat, time_sig_numerator));
                if let Some(ch) = inst.config.channel {
                    rec = rec.with_channel_filter(ch);
                }
                rec.record_note_on(note, velocity, channel, beat);
                inst.recording = rec;
                inst.phase = LooperPhase::Capturing {
                    last_note_off_beat: None,
                };
                actions.push(LooperAction::NoteOn {
                    voice_id,
                    note,
                    velocity,
                });
            }
            LooperPhase::Capturing { .. } => {
                inst.recording.record_note_on(note, velocity, channel, beat);
                actions.push(LooperAction::NoteOn {
                    voice_id,
                    note,
                    velocity,
                });
            }
            LooperPhase::Playing { pattern_id, .. } => {
                // Stop the current loop and start recording the next one.
                actions.push(LooperAction::StopPattern { pattern_id });
                inst.loop_count += 1;

                let mut rec =
                    MidiRecording::new(device_id, bar_aligned_start(beat, time_sig_numerator));
                if let Some(ch) = inst.config.channel {
                    rec = rec.with_channel_filter(ch);
                }
                rec.record_note_on(note, velocity, channel, beat);
                inst.recording = rec;
                inst.phase = LooperPhase::Capturing {
                    last_note_off_beat: None,
                };
                actions.push(LooperAction::NoteOn {
                    voice_id,
                    note,
                    velocity,
                });
            }
        }

        actions
    }

    /// Handle a note-off event for a device.
    pub fn handle_note_off(
        &mut self,
        device_id: MidiDeviceId,
        channel: u8,
        note: u8,
        current_beat: f64,
    ) -> Vec<LooperAction> {
        let Some(key) = self.key_for_event(device_id, channel) else {
            return Vec::new();
        };
        let Some(inst) = self.instances.get_mut(&key) else {
            return Vec::new();
        };

        let beat = Beat::from_f64(current_beat);
        let voice_id = inst.config.voice_id;

        if let LooperPhase::Capturing { .. } = &inst.phase {
            inst.recording.record_note_off(note, channel, beat);
            inst.phase = LooperPhase::Capturing {
                last_note_off_beat: Some(beat),
            };
        }

        vec![LooperAction::NoteOff { voice_id, note }]
    }

    /// Called every runtime tick. Detects silence and triggers playback conversion.
    pub fn tick(&mut self, current_beat: f64, time_sig_numerator: u8) -> Vec<LooperAction> {
        let mut actions = Vec::new();

        for inst in self.instances.values_mut() {
            let LooperPhase::Capturing { last_note_off_beat } = inst.phase else {
                continue;
            };

            let threshold = inst.config.silence_bars * time_sig_numerator as f64;

            // Don't count silence while the user is still holding keys.
            // `last_note_off_beat` only tracks the most recent release; if
            // the user lets one key go but keeps another held, the timer
            // would otherwise expire mid-performance and snap the loop
            // closed underneath them. Require *all* notes off for the
            // full silence window before finalising.
            let finalize_at = if inst.recording.pending_count() > 0 {
                // A note is still held with no note-off yet. Anchor the silence
                // timer to the last key *press* (absolute beat), not the
                // recording's start: with bar-aligned capture the start can sit
                // up to a full bar before the first note (a pickup), and using
                // it here would count that pre-note gap as silence and finalise
                // the loop underneath a held note.
                let last_press = inst.recording.last_note_on_beat().map(|rel| {
                    Beat::from_f64(inst.recording.start_beat.to_f64() + rel.to_f64())
                });
                let silence_anchor = last_note_off_beat
                    .or(last_press)
                    .unwrap_or(inst.recording.start_beat);
                let silence_beats = current_beat - silence_anchor.to_f64();
                if silence_beats < threshold * 2.0 {
                    continue;
                }

                let orphaned = inst
                    .recording
                    .close_pending_notes_at(Beat::from_f64(current_beat));
                tracing::warn!(
                    "Looper synthesized note-offs for orphaned pending notes: {:?}",
                    orphaned
                );
                actions.extend(
                    orphaned
                        .iter()
                        .map(|(note, _channel)| LooperAction::NoteOff {
                            voice_id: inst.config.voice_id,
                            note: *note,
                        }),
                );
                Beat::from_f64(current_beat)
            } else {
                let Some(last_off) = last_note_off_beat else {
                    continue;
                };
                let silence_beats = current_beat - last_off.to_f64();
                if silence_beats < threshold {
                    continue;
                }
                last_off
            };

            // Silence threshold reached — for normal captures, finalise at
            // the last note-off rather than baking the silence wait into the
            // loop. If pending notes were orphaned, the fallback synthesized
            // their note-offs at the current beat and finalises there.
            inst.recording.stop(finalize_at);

            // Quantize loop length to the nearest bar (minimum 1 bar).
            let bar_beats = time_sig_numerator as f64;
            let raw_duration = inst.recording.duration().to_f64();
            let bars_for_duration = (raw_duration / bar_beats).round();
            let bars_for_last_onset = inst
                .recording
                .last_note_on_beat()
                .map(|beat| (beat.to_f64() / bar_beats).ceil())
                .unwrap_or(1.0);
            let bars = bars_for_duration.max(bars_for_last_onset).max(1.0);
            let loop_length_beats = bars * bar_beats;

            // Build the pattern name and a stable ID from it.
            inst.loop_count += 1;
            let pattern_name = format!(
                "__looper_{}_{}_{}",
                inst.config.device_id.raw(),
                inst.config
                    .channel
                    .map(|ch| (ch as u16 + 1).to_string())
                    .unwrap_or_else(|| "all".to_string()),
                inst.loop_count
            );
            let pattern_id = name_to_pattern_id(&pattern_name);

            let config = inst.recording.to_pattern(
                &pattern_name,
                inst.config.quantize_beats,
                inst.config.voice_id,
                // Pass the bar-aligned loop length we just computed. The
                // pattern player wraps at this beat count; without it
                // matching a multiple of `bar_beats`, the rhythm drifts
                // off the bar grid every iteration.
                Beat::from_f64(loop_length_beats),
            );

            if let Err(err) = config.validate() {
                tracing::warn!(
                    "Looper generated invalid pattern {:?}: {:?}; returning to idle",
                    pattern_name,
                    err
                );
                inst.phase = LooperPhase::Idle;
                continue;
            }

            inst.phase = LooperPhase::Playing {
                pattern_id,
                loop_length_beats,
            };

            actions.push(LooperAction::StartPattern { config, pattern_id });
        }

        actions
    }
}

/// Bar line at or before `beat` for the given time signature — the anchor a
/// fresh capture starts from. Recording relative to the bar line (rather than
/// the first note-on) preserves each note's position *within the bar*, so the
/// captured loop lines up with the transport grid on playback instead of
/// forcing the first note onto a downbeat.
fn bar_aligned_start(beat: Beat, time_sig_numerator: u8) -> Beat {
    let bar_beats = (time_sig_numerator.max(1)) as f64;
    Beat::from_f64((beat.to_f64() / bar_beats).floor() * bar_beats)
}

/// Derive a stable `PatternId` from a string name via the standard hasher.
fn name_to_pattern_id(name: &str) -> PatternId {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    PatternId::new(hasher.finish() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reload::{LooperConfig, LooperKey};
    use crate::types::ids::{MidiDeviceId, VoiceId};

    fn make_config(device_id: u32, voice_id: u32) -> LooperConfig {
        LooperConfig {
            device_id: MidiDeviceId::new(device_id),
            voice_id: VoiceId::new(voice_id),
            channel: None,
            silence_bars: 1.0,
            quantize_beats: 0.25,
        }
    }

    fn setup(config: LooperConfig) -> (LooperManager, MidiDeviceId) {
        let device_id = config.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[config]);
        (manager, device_id)
    }

    fn phase(manager: &LooperManager, device_id: MidiDeviceId) -> LooperPhase {
        let mut matching = manager
            .instances
            .iter()
            .filter(|(key, _)| key.device_id == device_id)
            .map(|(_, instance)| instance.phase.clone());
        let phase = matching.next().unwrap();
        assert!(
            matching.next().is_none(),
            "phase(device_id) is ambiguous for multi-channel looper tests"
        );
        phase
    }

    fn phase_for(
        manager: &LooperManager,
        device_id: MidiDeviceId,
        channel: Option<u8>,
    ) -> LooperPhase {
        manager
            .instances
            .get(&LooperKey::new(device_id, channel))
            .unwrap()
            .phase
            .clone()
    }

    fn generated_pattern(actions: &[LooperAction]) -> &PatternConfig {
        actions
            .iter()
            .find_map(|action| match action {
                LooperAction::StartPattern { config, .. } => Some(config),
                _ => None,
            })
            .expect("looper should create a pattern")
    }

    fn record_single_note_loop(
        manager: &mut LooperManager,
        device_id: MidiDeviceId,
        note: u8,
        start_beat: f64,
    ) -> PatternId {
        manager.handle_note_on(device_id, 0, note, 100, start_beat, 4);
        manager.handle_note_off(device_id, 0, note, start_beat + 1.0);
        let actions = manager.tick(start_beat + 5.1, 4);
        actions
            .iter()
            .find_map(|action| match action {
                LooperAction::StartPattern { pattern_id, .. } => Some(*pattern_id),
                _ => None,
            })
            .expect("looper should start playback")
    }

    fn assert_pattern_contains_step(pattern: &PatternConfig, note: u8, beat: f64, gate: f32) {
        assert!(
            pattern.steps.iter().any(|step| {
                step.params.get("note") == Some(&(note as f32))
                    && (step.beat.to_f64() - beat).abs() < 0.001
                    && (step.params["gate"] - gate).abs() < 0.001
            }),
            "missing note {note} at beat {beat} with gate {gate}; steps: {:?}",
            pattern.steps
        );
    }

    #[test]
    fn test_idle_to_capturing_on_note_on() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        let actions = manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);

        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOn { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Capturing { .. }
        ));
    }

    #[test]
    fn test_capturing_stays_on_more_notes() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        let actions = manager.handle_note_on(device_id, 0, 64, 100, 1.0, 4);

        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOn { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Capturing { .. }
        ));
    }

    #[test]
    fn test_note_off_updates_last_note_off_beat() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        let actions = manager.handle_note_off(device_id, 0, 60, 2.0);

        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOff { .. })));
        if let LooperPhase::Capturing {
            last_note_off_beat: Some(beat),
        } = phase(&manager, device_id)
        {
            assert!((beat.to_f64() - 2.0).abs() < 0.001);
        } else {
            panic!("expected Capturing with last_note_off_beat set");
        }
    }

    #[test]
    fn test_no_silence_trigger_before_threshold() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        // note-off at beat 4.0; threshold = 1.0 bar * 4 beats = 4.0
        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 4.0);

        // silence = 7.9 - 4.0 = 3.9 < 4.0 — must not trigger
        let actions = manager.tick(7.9, 4);
        assert!(!actions
            .iter()
            .any(|a| matches!(a, LooperAction::StartPattern { .. })));
    }

    #[test]
    fn test_silence_triggers_playback_after_one_bar() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        // note-off at beat 4.0; threshold = 4.0 beats (1 bar in 4/4)
        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 4.0);

        // silence = 8.1 - 4.0 = 4.1 >= 4.0 — must trigger
        let actions = manager.tick(8.1, 4);
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::StartPattern { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Playing { .. }
        ));
    }

    #[test]
    fn stuck_pending_notes_are_synthesized_after_extended_silence() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.25,
        };
        let (mut manager, device_id) = setup(config);

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);

        assert!(manager.tick(1.9, 4).is_empty());

        let actions = manager.tick(2.1, 4);
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOff { note: 60, .. })));
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::StartPattern { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Playing { .. }
        ));
    }

    #[test]
    fn quantized_looper_preserves_retriggered_same_pitch_notes() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.125,
        };
        let (mut manager, device_id) = setup(config);

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_on(device_id, 0, 60, 110, 0.125, 4);
        manager.handle_note_off(device_id, 0, 60, 0.25);

        let actions = manager.tick(1.3, 4);
        let pattern = actions
            .iter()
            .find_map(|action| match action {
                LooperAction::StartPattern { config, .. } => Some(config),
                _ => None,
            })
            .expect("looper should create a pattern");

        assert_eq!(pattern.steps.len(), 2);
        assert!((pattern.steps[0].beat.to_f64() - 0.0).abs() < 0.001);
        assert!((pattern.steps[1].beat.to_f64() - 0.125).abs() < 0.001);
        assert!((pattern.steps[0].params["gate"] - 0.125).abs() < 0.001);
        assert!((pattern.steps[1].params["gate"] - 0.125).abs() < 0.001);
    }

    #[test]
    fn generated_pattern_preserves_performed_notes_under_realistic_quantized_capture() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.125,
        };
        let (mut manager, device_id) = setup(config);

        manager.handle_note_on(device_id, 0, 60, 100, 0.000, 4);
        manager.handle_note_on(device_id, 0, 64, 90, 0.030, 4);
        manager.handle_note_off(device_id, 0, 64, 0.055);
        manager.handle_note_on(device_id, 0, 60, 110, 0.125, 4);
        manager.handle_note_on(device_id, 0, 60, 120, 0.250, 4);
        manager.handle_note_off(device_id, 0, 60, 0.375);
        manager.handle_note_on(device_id, 0, 67, 80, 0.500, 4);
        manager.handle_note_on(device_id, 0, 69, 85, 0.560, 4);
        manager.handle_note_off(device_id, 0, 69, 0.600);
        manager.handle_note_off(device_id, 0, 67, 0.625);

        let actions = manager.tick(1.700, 4);
        let pattern = generated_pattern(&actions);

        assert_eq!(pattern.voice, Some(VoiceId::new(1)));
        assert!((pattern.length.to_f64() - 4.0).abs() < 0.001);
        assert_eq!(pattern.steps.len(), 6);

        assert_pattern_contains_step(pattern, 60, 0.000, 0.125);
        assert_pattern_contains_step(pattern, 64, 0.000, 0.100);
        assert_pattern_contains_step(pattern, 60, 0.125, 0.125);
        assert_pattern_contains_step(pattern, 60, 0.250, 0.125);
        assert_pattern_contains_step(pattern, 67, 0.500, 0.125);
        assert_pattern_contains_step(pattern, 69, 0.500, 0.125);
    }

    #[test]
    fn loop_length_covers_tail_notes_past_rounded_down_bar() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.125,
        };
        let (mut manager, device_id) = setup(config);

        manager.handle_note_on(device_id, 0, 60, 100, 0.000, 4);
        manager.handle_note_off(device_id, 0, 60, 0.250);
        manager.handle_note_on(device_id, 0, 64, 100, 4.500, 4);
        manager.handle_note_off(device_id, 0, 64, 4.750);
        manager.handle_note_on(device_id, 0, 67, 100, 4.875, 4);
        manager.handle_note_off(device_id, 0, 67, 5.000);

        let actions = manager.tick(6.100, 4);
        let pattern = generated_pattern(&actions);

        assert!((pattern.length.to_f64() - 8.0).abs() < 0.001);
        assert_eq!(pattern.steps.len(), 3);
        assert_pattern_contains_step(pattern, 60, 0.000, 0.250);
        assert_pattern_contains_step(pattern, 64, 4.500, 0.250);
        assert_pattern_contains_step(pattern, 67, 4.875, 0.125);

        if let LooperPhase::Playing {
            loop_length_beats, ..
        } = phase(&manager, device_id)
        {
            assert!(
                (loop_length_beats - 8.0).abs() < 0.001,
                "expected 8.0, got {loop_length_beats}"
            );
        } else {
            panic!("expected Playing phase");
        }
    }

    #[test]
    fn test_note_on_during_playback_restarts_capture() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 4.0);
        manager.tick(8.1, 4);
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Playing { .. }
        ));

        let actions = manager.handle_note_on(device_id, 0, 62, 100, 9.0, 4);

        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::StopPattern { .. })));
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOn { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Capturing { .. }
        ));
    }

    #[test]
    fn test_reconcile_removes_playing_looper() {
        let (mut manager, device_id) = setup(make_config(1, 1));

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 4.0);
        manager.tick(8.1, 4);
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Playing { .. }
        ));

        let actions = manager.reconcile(&[]);
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::StopPattern { .. })));
        assert!(!manager.has_device(device_id));
    }

    #[test]
    fn removing_one_looper_config_stops_only_that_loopers_pattern() {
        let config_a = make_config(1, 1);
        let config_b = make_config(2, 2);
        let device_a = config_a.device_id;
        let device_b = config_b.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[config_a, config_b.clone()]);

        let pattern_a = record_single_note_loop(&mut manager, device_a, 60, 0.0);
        let pattern_b = record_single_note_loop(&mut manager, device_b, 67, 8.0);

        let actions = manager.reconcile(&[config_b]);

        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, LooperAction::StopPattern { .. }))
                .count(),
            1
        );
        assert!(actions.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_a)
        ));
        assert!(!actions.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_b)
        ));
        assert!(!manager.has_device(device_a));
        assert!(matches!(
            phase(&manager, device_b),
            LooperPhase::Playing { pattern_id, .. } if pattern_id == pattern_b
        ));
    }

    #[test]
    fn note_on_during_playback_stops_only_that_devices_pattern() {
        let config_a = make_config(1, 1);
        let config_b = make_config(2, 2);
        let device_a = config_a.device_id;
        let device_b = config_b.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[config_a, config_b]);

        let pattern_a = record_single_note_loop(&mut manager, device_a, 60, 0.0);
        let pattern_b = record_single_note_loop(&mut manager, device_b, 67, 8.0);

        let actions = manager.handle_note_on(device_a, 0, 62, 100, 14.0, 4);

        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, LooperAction::StopPattern { .. }))
                .count(),
            1
        );
        assert!(actions.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_a)
        ));
        assert!(!actions.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_b)
        ));
        assert!(matches!(
            phase(&manager, device_a),
            LooperPhase::Capturing { .. }
        ));
        assert!(matches!(
            phase(&manager, device_b),
            LooperPhase::Playing { pattern_id, .. } if pattern_id == pattern_b
        ));
    }

    #[test]
    fn same_device_distinct_channel_loopers_keep_independent_patterns_across_reconcile() {
        let mut config_a = make_config(1, 1);
        config_a.channel = Some(0);
        let mut config_b = make_config(1, 2);
        config_b.channel = Some(1);
        let device_id = config_a.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[config_a.clone(), config_b.clone()]);

        let pattern_a = record_single_note_loop(&mut manager, device_id, 60, 0.0);
        let pattern_b = {
            manager.handle_note_on(device_id, 1, 67, 100, 8.0, 4);
            manager.handle_note_off(device_id, 1, 67, 9.0);
            manager
                .tick(13.1, 4)
                .iter()
                .find_map(|action| match action {
                    LooperAction::StartPattern { pattern_id, .. } => Some(*pattern_id),
                    _ => None,
                })
                .expect("second channel looper should start playback")
        };

        assert_ne!(pattern_a, pattern_b);

        for _ in 0..3 {
            let actions = manager.reconcile(&[config_a.clone(), config_b.clone()]);
            assert!(
                actions.is_empty(),
                "unchanged multi-channel loopers should reconcile without actions: {actions:?}"
            );
        }

        let restart_a = manager.handle_note_on(device_id, 0, 62, 100, 16.0, 4);
        assert_eq!(
            restart_a
                .iter()
                .filter(|action| matches!(action, LooperAction::StopPattern { .. }))
                .count(),
            1
        );
        assert!(restart_a.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_a)
        ));
        assert!(!restart_a.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_b)
        ));

        let remove_b = manager.reconcile(&[config_a]);
        assert_eq!(
            remove_b
                .iter()
                .filter(|action| matches!(action, LooperAction::StopPattern { .. }))
                .count(),
            1
        );
        assert!(remove_b.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id } if *pattern_id == pattern_b)
        ));
        assert!(matches!(
            phase_for(&manager, device_id, Some(0)),
            LooperPhase::Capturing { .. }
        ));
        assert!(!manager
            .instances
            .contains_key(&LooperKey::new(device_id, Some(1))));
    }

    #[test]
    fn channel_specific_looper_takes_precedence_over_same_device_all_channel_looper() {
        let mut all_channels = make_config(1, 1);
        all_channels.channel = None;
        let mut channel_one = make_config(1, 2);
        channel_one.channel = Some(0);
        let device_id = all_channels.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[all_channels, channel_one]);

        let actions = manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);

        assert!(actions.iter().any(
            |action| matches!(action, LooperAction::NoteOn { voice_id, .. } if *voice_id == VoiceId::new(2))
        ));
        assert!(matches!(
            phase_for(&manager, device_id, Some(0)),
            LooperPhase::Capturing { .. }
        ));
        assert!(matches!(
            phase_for(&manager, device_id, None),
            LooperPhase::Idle
        ));
    }

    #[test]
    fn changed_looper_config_stops_active_pattern_and_resets_instance() {
        let mut config = make_config(1, 1);
        config.channel = Some(0);
        let device_id = config.device_id;
        let mut manager = LooperManager::new();
        manager.reconcile(&[config.clone()]);
        let pattern_id = record_single_note_loop(&mut manager, device_id, 60, 0.0);

        let mut changed = config;
        changed.quantize_beats = 0.5;
        let actions = manager.reconcile(&[changed.clone()]);

        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, LooperAction::StopPattern { .. }))
                .count(),
            1
        );
        assert!(actions.iter().any(
            |action| matches!(action, LooperAction::StopPattern { pattern_id: stopped } if *stopped == pattern_id)
        ));
        assert!(matches!(
            phase_for(&manager, device_id, Some(0)),
            LooperPhase::Idle
        ));

        let actions = manager.handle_note_on(device_id, 0, 62, 100, 9.0, 4);
        assert!(actions.iter().any(
            |action| matches!(action, LooperAction::NoteOn { voice_id, .. } if *voice_id == changed.voice_id)
        ));
    }

    #[test]
    fn test_loop_length_quantized_to_bar() {
        // silence_bars=0.25 → threshold = 0.25 * 4 = 1.0 beat
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.25,
        };
        let (mut manager, device_id) = setup(config);

        // Recording from beat 0.0 to note-off at 3.5
        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 3.5);

        // Tick at 4.6: silence = 1.1 >= 1.0; raw_duration = 4.6; bars = round(1.15) = 1; loop = 4.0
        let actions = manager.tick(4.6, 4);
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::StartPattern { .. })));
        if let LooperPhase::Playing {
            loop_length_beats, ..
        } = phase(&manager, device_id)
        {
            assert!(
                (loop_length_beats - 4.0).abs() < 0.001,
                "expected 4.0, got {loop_length_beats}"
            );
        } else {
            panic!("expected Playing phase");
        }
    }

    #[test]
    fn invalid_generated_pattern_does_not_enter_playing_phase() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: None,
            silence_bars: 0.25,
            quantize_beats: 0.25,
        };
        let (mut manager, device_id) = setup(config);

        manager.handle_note_on(device_id, 0, 60, 100, 0.0, 4);
        manager.handle_note_off(device_id, 0, 60, 70_000.0);

        let actions = manager.tick(70_002.0, 4);

        assert!(!actions
            .iter()
            .any(|a| matches!(a, LooperAction::StartPattern { .. })));
        assert!(matches!(phase(&manager, device_id), LooperPhase::Idle));
    }

    #[test]
    fn test_channel_filter_ignored_when_none() {
        let (mut manager, device_id) = setup(make_config(1, 1)); // channel = None

        // note on channel 3 — must still be handled
        let actions = manager.handle_note_on(device_id, 3, 60, 100, 0.0, 4);
        assert!(actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOn { .. })));
        assert!(matches!(
            phase(&manager, device_id),
            LooperPhase::Capturing { .. }
        ));
    }

    #[test]
    fn test_channel_filter_blocks_other_channels() {
        let config = LooperConfig {
            device_id: MidiDeviceId::new(1),
            voice_id: VoiceId::new(1),
            channel: Some(0), // only channel 0
            silence_bars: 1.0,
            quantize_beats: 0.25,
        };
        let (mut manager, device_id) = setup(config);

        // note on channel 1 — must be blocked
        let actions = manager.handle_note_on(device_id, 1, 60, 100, 0.0, 4);
        assert!(!actions
            .iter()
            .any(|a| matches!(a, LooperAction::NoteOn { .. })));
        assert!(matches!(phase(&manager, device_id), LooperPhase::Idle));
    }
}
