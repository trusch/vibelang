//! Voice management handlers.
//!
//! Handles voice-related messages:
//! - Voice creation/update (UpsertVoice)
//! - Voice deletion
//! - Voice parameter changes
//! - Voice mute/unmute

use super::RuntimeContext;
use crate::osc_sender::OscTiming;
use crate::scsynth::NodeId;
use crate::state::{SourceLocation, VoiceState};
use std::collections::HashMap;
use std::time::Instant;

/// Handle UpsertVoice message - create or update a voice.
#[allow(clippy::too_many_arguments)]
pub fn handle_upsert_voice(
    ctx: &mut RuntimeContext<'_>,
    name: String,
    group_path: String,
    group_name: Option<String>,
    synth_name: Option<String>,
    polyphony: i64,
    gain: f64,
    muted: bool,
    soloed: bool,
    output_bus: Option<i64>,
    params: HashMap<String, f32>,
    sfz_instrument: Option<String>,
    vst_instrument: Option<String>,
    source_location: SourceLocation,
    midi_output_device_id: Option<u32>,
    midi_channel: Option<u8>,
    midi_note: Option<u8>,
    cc_mappings: HashMap<String, u8>,
) {
    let generation = ctx.shared.with_state_read(|s| s.reload_generation);

    // Check if gain changed and get running node if any
    let (gain_changed, running_node) = ctx.shared.with_state_read(|state| {
        if let Some(voice) = state.voices.get(&name) {
            let changed = (voice.gain - gain).abs() > 0.0001;
            (changed, voice.running_node_id)
        } else {
            (false, None)
        }
    });

    ctx.shared.with_state_write(|state| {
        let voice = state
            .voices
            .entry(name.clone())
            .or_insert_with(|| VoiceState::new(name.clone(), group_path.clone()));
        voice.group_path = group_path;
        voice.group_name = group_name;
        voice.synth_name = synth_name;
        voice.polyphony = polyphony;
        voice.gain = gain;
        voice.muted = muted;
        voice.soloed = soloed;
        voice.output_bus = output_bus;
        voice.params = params;
        voice.sfz_instrument = sfz_instrument;
        voice.vst_instrument = vst_instrument;
        voice.generation = generation;
        voice.source_location = source_location;
        voice.midi_output_device_id = midi_output_device_id;
        voice.midi_channel = midi_channel;
        voice.midi_note = midi_note;
        voice.cc_mappings = cc_mappings;
        state.bump_version();
    });

    // If gain changed and voice has a running synth, update it
    if gain_changed {
        if let Some(node_id) = running_node {
            let current_beat = ctx.transport.beat_at(Instant::now()).to_float();
            let _ = ctx.osc_sender.n_set(
                OscTiming::Now,
                NodeId::new(node_id),
                &[("amp", gain as f32)],
                current_beat,
            );
            log::debug!(
                "[VOICE] Updated running node {} gain to {}",
                node_id,
                gain
            );
        }
    }
}

/// Handle DeleteVoice message - remove a voice from state.
pub fn handle_delete_voice(ctx: &mut RuntimeContext<'_>, name: String) {
    ctx.shared.with_state_write(|state| {
        state.voices.remove(&name);
        state.bump_version();
    });
}

/// Handle SetVoiceParam message - set a parameter on a voice.
pub fn handle_set_voice_param(ctx: &mut RuntimeContext<'_>, name: &str, param: String, value: f32) {
    // Check if this voice has MIDI CC mapping for this param
    let midi_cc_info = ctx.shared.with_state_read(|state| {
        if let Some(voice) = state.voices.get(name) {
            if let Some(device_id) = voice.midi_output_device_id {
                if let Some(&cc_num) = voice.cc_mappings.get(&param) {
                    let channel = voice.midi_channel.unwrap_or(0);
                    if let Some(device) = state.midi_output_config.devices.get(&device_id) {
                        return Some((device.event_tx.clone(), channel, cc_num));
                    }
                }
            }
        }
        None
    });

    // Send MIDI CC if mapped
    if let Some((event_tx, channel, cc_num)) = midi_cc_info {
        // Convert 0.0-1.0 to 0-127
        let cc_value = (value.clamp(0.0, 1.0) * 127.0) as u8;
        let midi_event = crate::midi::QueuedMidiEvent::control_change(channel, cc_num, cc_value);
        let _ = event_tx.send(midi_event);
        log::debug!(
            "[MIDI_OUT] Voice '{}' CC: {}={} (param='{}', ch={})",
            name,
            cc_num,
            cc_value,
            param,
            channel + 1
        );
    }

    // Always update the local state too
    ctx.shared.with_state_write(|state| {
        if let Some(voice) = state.voices.get_mut(name) {
            voice.params.insert(param, value);
            state.bump_version();
        }
    });
}

/// Handle MuteVoice message - mute a voice.
pub fn handle_mute_voice(ctx: &mut RuntimeContext<'_>, name: &str) {
    ctx.shared.with_state_write(|state| {
        if let Some(voice) = state.voices.get_mut(name) {
            voice.muted = true;
            state.bump_version();
        }
    });
}

/// Handle UnmuteVoice message - unmute a voice.
pub fn handle_unmute_voice(ctx: &mut RuntimeContext<'_>, name: &str) {
    ctx.shared.with_state_write(|state| {
        if let Some(voice) = state.voices.get_mut(name) {
            voice.muted = false;
            state.bump_version();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi_realtime::MidiRealtimeService;
    use crate::osc_sender::OscSender;
    use crate::reload::ReloadManager;
    use crate::scheduler::EventScheduler;
    use crate::scsynth::Scsynth;
    use crate::state::StateManager;
    use crate::timing::TransportClock;

    fn create_test_context() -> (
        OscSender,
        Scsynth,
        StateManager,
        EventScheduler,
        TransportClock,
        ReloadManager,
        Option<crossbeam_channel::Sender<String>>,
        Option<i32>,
        MidiRealtimeService,
    ) {
        let sc = Scsynth::noop();
        let osc_sender = OscSender::new(sc.clone());
        let shared = StateManager::new();
        let scheduler = EventScheduler::new();
        let transport = TransportClock::new();
        let reload_manager = ReloadManager::new();
        let completion_tx = None;
        let sc_midi_clock_node_id = None;
        let midi_rt = MidiRealtimeService::noop();

        (
            osc_sender,
            sc,
            shared,
            scheduler,
            transport,
            reload_manager,
            completion_tx,
            sc_midi_clock_node_id,
            midi_rt,
        )
    }

    #[test]
    fn test_handle_upsert_voice_creates_new() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        handle_upsert_voice(
            &mut ctx,
            "kick".to_string(),
            "main/drums".to_string(),
            Some("drums".to_string()),
            Some("default".to_string()),
            4i64,
            0.8,
            false,
            false,
            None,
            HashMap::new(),
            None,
            None,
            SourceLocation::default(),
            None,
            None,
            None,
            HashMap::new(),
        );

        let voice_exists = shared.with_state_read(|s| s.voices.contains_key("kick"));
        assert!(voice_exists);

        let gain = shared.with_state_read(|s| s.voices.get("kick").map(|v| v.gain));
        assert_eq!(gain, Some(0.8));
    }

    #[test]
    fn test_handle_delete_voice() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        // Create a voice first
        shared.with_state_write(|state| {
            let voice = VoiceState::new("test_voice".to_string(), "main".to_string());
            state.voices.insert("test_voice".to_string(), voice);
        });

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        assert!(shared.with_state_read(|s| s.voices.contains_key("test_voice")));

        handle_delete_voice(&mut ctx, "test_voice".to_string());

        assert!(!shared.with_state_read(|s| s.voices.contains_key("test_voice")));
    }

    #[test]
    fn test_handle_mute_unmute_voice() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        // Create a voice first
        shared.with_state_write(|state| {
            let voice = VoiceState::new("test_voice".to_string(), "main".to_string());
            state.voices.insert("test_voice".to_string(), voice);
        });

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        // Initially not muted
        assert!(!shared.with_state_read(|s| s.voices.get("test_voice").map(|v| v.muted).unwrap_or(true)));

        handle_mute_voice(&mut ctx, "test_voice");
        assert!(shared.with_state_read(|s| s.voices.get("test_voice").map(|v| v.muted).unwrap_or(false)));

        handle_unmute_voice(&mut ctx, "test_voice");
        assert!(!shared.with_state_read(|s| s.voices.get("test_voice").map(|v| v.muted).unwrap_or(true)));
    }

    #[test]
    fn test_handle_set_voice_param() {
        let (
            mut osc_sender,
            sc,
            shared,
            mut scheduler,
            mut transport,
            mut reload_manager,
            completion_tx,
            mut sc_midi_clock_node_id,
            mut midi_rt,
        ) = create_test_context();

        // Create a voice first
        shared.with_state_write(|state| {
            let voice = VoiceState::new("test_voice".to_string(), "main".to_string());
            state.voices.insert("test_voice".to_string(), voice);
        });

        let mut ctx = RuntimeContext {
            osc_sender: &mut osc_sender,
            sc: &sc,
            shared: &shared,
            scheduler: &mut scheduler,
            transport: &mut transport,
            reload_manager: &mut reload_manager,
            completion_tx: &completion_tx,
            sc_midi_clock_node_id: &mut sc_midi_clock_node_id,
            midi_rt: &mut midi_rt,
        };

        handle_set_voice_param(&mut ctx, "test_voice", "cutoff".to_string(), 0.75);

        let cutoff = shared.with_state_read(|s| {
            s.voices.get("test_voice").and_then(|v| v.params.get("cutoff").copied())
        });
        assert_eq!(cutoff, Some(0.75));
    }
}
