//! Voice API for Rhai scripts.
//!
//! Voices are the basic sound-producing units in VibeLang.

use crate::state::StateMessage;
use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;
use vibelang_sfz::SfzInstrumentHandle;

use super::context::{self, SourceLocation};
use super::midi::MidiDevice;
use super::require_handle;

/// A Voice builder for creating and configuring voices.
#[derive(Debug, Clone, CustomType)]
pub struct Voice {
    /// Voice name.
    pub name: String,
    /// SynthDef name.
    synth_name: Option<String>,
    /// Group path.
    group_path: String,
    /// Polyphony (number of simultaneous voices).
    polyphony: i64,
    /// Gain in linear amplitude.
    gain: f64,
    /// Default parameters.
    params: HashMap<String, f64>,
    /// Whether the voice is muted.
    muted: bool,
    /// Whether the voice is soloed.
    soloed: bool,
    /// SFZ instrument ID (if using SFZ).
    sfz_instrument: Option<String>,
    /// Source location where this voice was defined.
    source_location: SourceLocation,
    /// MIDI output device ID (if routing to external MIDI hardware).
    midi_output_device_id: Option<u32>,
    /// MIDI channel for output (1-16, converted to 0-15 internally).
    midi_channel: Option<u8>,
    /// CC mappings: parameter_name -> CC number.
    cc_mappings: HashMap<String, u8>,
}

impl Voice {
    /// Create a new voice with the given name and source location from NativeCallContext.
    pub fn new(ctx: NativeCallContext, name: String) -> Self {
        let pos = ctx.call_position();
        let source_location = SourceLocation::new(
            context::get_current_script_file(),
            if pos.is_none() { None } else { pos.line().map(|l| l as u32) },
            if pos.is_none() { None } else { pos.position().map(|c| c as u32) },
        );
        Self {
            name,
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            sfz_instrument: None,
            source_location,
            midi_output_device_id: None,
            midi_channel: None,
            cc_mappings: HashMap::new(),
        }
    }

    // === Getters ===

    /// Get the voice ID.
    pub fn id(&mut self) -> String {
        self.name.clone()
    }

    /// Get the voice name.
    pub fn get_name(&mut self) -> String {
        self.name.clone()
    }

    /// Get the synth name.
    pub fn get_synth_name(&mut self) -> String {
        self.synth_name.clone().unwrap_or_default()
    }

    /// Get the gain.
    pub fn get_gain(&mut self) -> f64 {
        self.gain
    }

    /// Get the polyphony.
    pub fn get_polyphony(&mut self) -> i64 {
        self.polyphony
    }

    /// Get the group path.
    pub fn get_group_path(&mut self) -> String {
        self.group_path.clone()
    }

    /// Check if muted.
    pub fn is_muted(&mut self) -> bool {
        self.muted
    }

    /// Check if soloed.
    pub fn is_soloed(&mut self) -> bool {
        self.soloed
    }

    // === Builder methods (return self for chaining) ===

    /// Set the group for this voice.
    pub fn group(mut self, group: String) -> Self {
        self.group_path = if group.starts_with("main/") || group == "main" {
            group
        } else {
            format!("{}/{}", context::current_group_path(), group)
        };
        self.sync_state();
        self
    }

    /// Set the synth for this voice (alias for `on`).
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self.sync_state();
        self
    }

    /// Set the sound source (synthdef name).
    pub fn on(mut self, source: String) -> Self {
        self.synth_name = Some(source);
        self.sync_state();
        self
    }

    /// Set the sound source to an SFZ instrument.
    pub fn on_sfz(mut self, sfz: SfzInstrumentHandle) -> Self {
        self.sfz_instrument = Some(sfz.id.clone());
        // Use sfz_voice synthdef for SFZ playback
        self.synth_name = Some("sfz_voice".to_string());
        self.sync_state();
        self
    }

    /// Set the sound source to a sample.
    ///
    /// The sample's envelope, offset, rate, and other parameters will be used
    /// as defaults when this voice is triggered.
    ///
    /// If the sample has warp_mode enabled, uses the warp_voice synthdef for
    /// time-stretching and pitch-shifting. Otherwise uses sample_voice for
    /// standard playback.
    pub fn on_sample(mut self, sample: super::sample::SampleHandle) -> Self {
        log::debug!(
            "[VOICE] on_sample called for voice '{}' with sample '{}' (warp_mode={})",
            self.name,
            sample.id,
            sample.warp_mode
        );

        let num_channels = sample.num_channels();

        if sample.warp_mode {
            // Use warp_voice synthdef for time-stretching/pitch-shifting
            let synthdef_name = if num_channels == 1 {
                "warp_voice_mono".to_string()
            } else {
                "warp_voice_stereo".to_string()
            };
            self.synth_name = Some(synthdef_name);

            // Set warp parameters
            self.params.insert("bufnum".to_string(), sample.buffer_id() as f64);
            self.params.insert("speed".to_string(), sample.speed);
            self.params.insert("pitch".to_string(), sample.pitch);
            self.params.insert("amp".to_string(), sample.amp);
            self.params.insert("attack".to_string(), sample.attack);
            self.params.insert("sustain".to_string(), sample.sustain_level);
            self.params.insert("release".to_string(), sample.release);
            self.params.insert("windowSize".to_string(), sample.window_size);
            self.params.insert("overlaps".to_string(), sample.overlaps);

            // Warp uses normalized positions (0-1)
            let duration = sample.duration();
            if duration > 0.0 {
                let start_pos = sample.offset_seconds / duration;
                let end_pos = if let Some(length) = sample.length_seconds {
                    ((sample.offset_seconds + length) / duration).min(1.0)
                } else {
                    1.0
                };
                self.params.insert("startPos".to_string(), start_pos);
                self.params.insert("endPos".to_string(), end_pos);
            } else {
                self.params.insert("startPos".to_string(), 0.0);
                self.params.insert("endPos".to_string(), 1.0);
            }

            log::info!(
                "[VOICE] Using warp_voice for '{}' (speed={:.2}, pitch={:.2})",
                self.name,
                sample.speed,
                sample.pitch
            );
        } else {
            // Use standard sample_voice synthdef
            let synthdef_name = if num_channels == 1 {
                "sample_voice_mono".to_string()
            } else {
                "sample_voice_stereo".to_string()
            };
            self.synth_name = Some(synthdef_name);

            // Set the sample's parameters as defaults for the voice
            self.params.insert("bufnum".to_string(), sample.buffer_id() as f64);
            self.params.insert("attack".to_string(), sample.attack);
            self.params.insert("sustain".to_string(), sample.sustain_level);
            self.params.insert("release".to_string(), sample.release);
            self.params.insert("rate".to_string(), sample.rate);
            self.params.insert("loop".to_string(), if sample.loop_mode { 1.0 } else { 0.0 });
            self.params.insert("amp".to_string(), sample.amp);
            self.params.insert("startPos".to_string(), sample.get_start_frame() as f64);

            let end_frame = sample.get_end_frame();
            if end_frame > 0 {
                self.params.insert("endPos".to_string(), end_frame as f64);
            }

            log::info!(
                "[VOICE] Using sample_voice for '{}' (buffer={}, rate={:.2})",
                self.name,
                sample.buffer_id(),
                sample.rate
            );
        }

        self.sync_state();
        self
    }

    /// Set the sound source to a MIDI output device.
    ///
    /// When a voice is routed to a MIDI output device, note and parameter
    /// events will be sent as MIDI messages instead of SuperCollider commands.
    ///
    /// # Example
    /// ```rhai
    /// let midi_out = midi_open("Model 15", "output");
    /// let lead = voice("lead")
    ///     .on(midi_out)
    ///     .channel(1)
    ///     .cc(74, "filter")
    ///     .apply();
    /// ```
    pub fn on_midi(mut self, device: MidiDevice) -> Result<Self, Box<EvalAltResult>> {
        let device_id = device.output_device_id.ok_or_else(|| {
            Box::new(EvalAltResult::from(
                "MIDI device was not opened for output. Use midi_open(\"name\", \"output\") or midi_open(\"name\", \"both\")"
            ))
        })?;
        self.midi_output_device_id = Some(device_id);
        // Clear synth_name since we're routing to MIDI, not SuperCollider
        self.synth_name = None;
        self.sfz_instrument = None;
        log::info!(
            "[VOICE] Routing voice '{}' to MIDI output device '{}'",
            self.name,
            device.name
        );
        self.sync_state();
        Ok(self)
    }

    /// Set the MIDI channel for this voice (1-16).
    ///
    /// Must be used with `.on(midi_output)` for MIDI output routing.
    pub fn channel(mut self, ch: i64) -> Self {
        // Convert 1-16 to 0-15 internally
        self.midi_channel = Some((ch.clamp(1, 16) - 1) as u8);
        self.sync_state();
        self
    }

    /// Map a CC number to a parameter name for MIDI output.
    ///
    /// When `.set("param_name", value)` is called on a MIDI voice,
    /// the mapped CC message will be sent instead of a SuperCollider command.
    ///
    /// # Example
    /// ```rhai
    /// let lead = voice("lead")
    ///     .on(midi_out)
    ///     .channel(1)
    ///     .cc(74, "filter")      // Map CC#74 to "filter" param
    ///     .cc(71, "resonance")   // Map CC#71 to "resonance" param
    ///     .apply();
    ///
    /// lead.set("filter", 0.5);     // Sends CC#74 with value 64
    /// lead.set("resonance", 0.8);  // Sends CC#71 with value 102
    /// ```
    pub fn cc(mut self, cc_num: i64, param_name: String) -> Self {
        self.cc_mappings.insert(param_name, cc_num.clamp(0, 127) as u8);
        self.sync_state();
        self
    }

    /// Set the polyphony.
    pub fn poly(mut self, count: i64) -> Self {
        self.polyphony = count;
        self.sync_state();
        self
    }

    /// Set the gain.
    pub fn gain(mut self, value: f64) -> Self {
        self.gain = value;
        self.sync_state();
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value);
        self.sync_state();
        self
    }

    /// Mute the voice.
    pub fn mute(mut self) -> Self {
        self.muted = true;
        self.sync_state();
        self
    }

    /// Solo the voice.
    pub fn solo(mut self) -> Self {
        self.soloed = true;
        self.sync_state();
        self
    }

    /// Set the output bus.
    pub fn set_output_bus(self, _bus: i64) -> Self {
        // TODO: Implement output bus routing
        self
    }

    /// Run this voice continuously (for line-in, drones, etc.).
    ///
    /// Unlike melody/pattern triggers, this starts the synth immediately
    /// and keeps it running until stopped or the script is reloaded.
    ///
    /// # Example
    /// ```rhai
    /// let mic = voice("mic").synth("line_in").gain(db(-6));
    /// mic.run();  // Starts immediately
    /// ```
    pub fn run(self) -> Self {
        self.sync_state();
        let handle = require_handle();
        let _ = handle.send(StateMessage::RunVoice {
            name: self.name.clone(),
        });
        self
    }

    // === Actions ===

    /// Sync this voice's state with the runtime.
    fn sync_state(&self) {
        let handle = require_handle();

        // Convert params from f64 to f32
        let params: std::collections::HashMap<String, f32> = self
            .params
            .iter()
            .map(|(k, v)| (k.clone(), *v as f32))
            .collect();

        let _ = handle.send(StateMessage::UpsertVoice {
            name: self.name.clone(),
            group_path: self.group_path.clone(),
            group_name: None,
            synth_name: self.synth_name.clone(),
            polyphony: self.polyphony,
            gain: self.gain,
            muted: self.muted,
            soloed: self.soloed,
            output_bus: None,
            params,
            sfz_instrument: self.sfz_instrument.clone(),
            vst_instrument: None,
            source_location: self.source_location.clone(),
            midi_output_device_id: self.midi_output_device_id,
            midi_channel: self.midi_channel,
            cc_mappings: self.cc_mappings.clone(),
        });
    }

    /// Register this voice with the runtime (explicit call, same as sync_state).
    /// Returns self for chaining.
    pub fn apply(self) -> Self {
        let handle = require_handle();

        // Convert params from f64 to f32
        let params: std::collections::HashMap<String, f32> = self
            .params
            .iter()
            .map(|(k, v)| (k.clone(), *v as f32))
            .collect();

        let _ = handle.send(StateMessage::UpsertVoice {
            name: self.name.clone(),
            group_path: self.group_path.clone(),
            group_name: None,
            synth_name: self.synth_name.clone(),
            polyphony: self.polyphony,
            gain: self.gain,
            muted: self.muted,
            soloed: self.soloed,
            output_bus: None,
            params,
            sfz_instrument: self.sfz_instrument.clone(),
            vst_instrument: None,
            source_location: self.source_location.clone(),
            midi_output_device_id: self.midi_output_device_id,
            midi_channel: self.midi_channel,
            cc_mappings: self.cc_mappings.clone(),
        });

        self
    }
}

/// Create a new voice builder with source location tracking.
pub fn voice(ctx: NativeCallContext, name: String) -> Voice {
    Voice::new(ctx, name)
}

/// Trigger a voice with parameters.
pub fn voice_trigger(voice: &mut Voice, params: rhai::Map) {
    let handle = require_handle();
    let mut param_vec: Vec<(String, f32)> = Vec::new();

    for (key, value) in params {
        if let Ok(v) = value.as_float() {
            param_vec.push((key.to_string(), v as f32));
        } else if let Ok(v) = value.as_int() {
            param_vec.push((key.to_string(), v as f32));
        }
    }

    let _ = handle.send(StateMessage::TriggerVoice {
        name: voice.name.clone(),
        synth_name: voice.synth_name.clone(),
        group_path: Some(voice.group_path.clone()),
        params: param_vec,
    });
}

/// Trigger a voice without parameters.
pub fn voice_trigger_no_params(voice: &mut Voice) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::TriggerVoice {
        name: voice.name.clone(),
        synth_name: voice.synth_name.clone(),
        group_path: Some(voice.group_path.clone()),
        params: Vec::new(),
    });
}

/// Stop all sounds from a voice.
pub fn voice_stop(voice: &mut Voice) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::StopVoice {
        name: voice.name.clone(),
    });
}

/// Stop all sounds from all voices.
pub fn voice_stop_all(voice: &mut Voice) {
    // Just stop this voice - there's no StopAllVoices message
    voice_stop(voice);
}

/// Send note on.
pub fn voice_note_on(voice: &mut Voice, note: String, velocity: f64) {
    let midi_note = super::helpers::note(&note) as u8;
    let handle = require_handle();
    let _ = handle.send(StateMessage::NoteOn {
        voice_name: voice.name.clone(),
        note: midi_note,
        velocity: (velocity * 127.0) as u8,
        duration: None,
    });
}

/// Send note on (integer note).
pub fn voice_note_on_int(voice: &mut Voice, note: i64, velocity: f64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::NoteOn {
        voice_name: voice.name.clone(),
        note: note as u8,
        velocity: (velocity * 127.0) as u8,
        duration: None,
    });
}

/// Send note on (float note).
pub fn voice_note_on_float(voice: &mut Voice, note: f64, velocity: f64) {
    voice_note_on_int(voice, note as i64, velocity)
}

/// Send note on with integer velocity.
pub fn voice_note_on_int_vel(voice: &mut Voice, note: i64, velocity: i64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::NoteOn {
        voice_name: voice.name.clone(),
        note: note as u8,
        velocity: velocity as u8,
        duration: None,
    });
}

/// Send note on with float velocity.
pub fn voice_note_on_float_vel(voice: &mut Voice, note: f64, velocity: i64) {
    voice_note_on_int_vel(voice, note as i64, velocity)
}

/// Send note off.
pub fn voice_note_off(voice: &mut Voice, note: i64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::NoteOff {
        voice_name: voice.name.clone(),
        note: note as u8,
    });
}

/// Send control change.
pub fn voice_control_change(voice: &mut Voice, cc: i64, value: f64) {
    let handle = require_handle();
    let _ = handle.send(StateMessage::ControlChange {
        voice_name: voice.name.clone(),
        cc_num: cc as u8,
        value: (value * 127.0) as u8,
    });
}

/// Register voice API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Voice type
    engine.build_type::<Voice>();

    // Constructor
    engine.register_fn("voice", voice);

    // Getters
    engine.register_fn("id", Voice::id);
    engine.register_fn("name", Voice::get_name);
    engine.register_get("name", Voice::get_name);
    engine.register_fn("synth_name", Voice::get_synth_name);
    engine.register_get("synth_name", Voice::get_synth_name);
    engine.register_fn("get_gain", Voice::get_gain);
    engine.register_get("gain", Voice::get_gain);
    engine.register_fn("polyphony", Voice::get_polyphony);
    engine.register_get("polyphony", Voice::get_polyphony);
    engine.register_fn("group_path", Voice::get_group_path);
    engine.register_get("group_path", Voice::get_group_path);
    engine.register_fn("is_muted", Voice::is_muted);
    engine.register_get("muted", Voice::is_muted);
    engine.register_fn("is_soloed", Voice::is_soloed);
    engine.register_get("soloed", Voice::is_soloed);

    // Builder methods
    engine.register_fn("group", Voice::group);
    engine.register_fn("synth", Voice::synth);
    engine.register_fn("on", Voice::on);
    engine.register_fn("on", Voice::on_sfz);     // SFZ overload
    engine.register_fn("on", Voice::on_sample);  // Sample overload
    engine.register_fn("on", Voice::on_midi);    // MIDI output overload
    engine.register_fn("channel", Voice::channel);
    engine.register_fn("cc", Voice::cc);
    engine.register_fn("poly", Voice::poly);
    engine.register_fn("gain", Voice::gain);
    engine.register_fn("set_param", Voice::set_param);
    engine.register_fn("mute", Voice::mute);
    engine.register_fn("solo", Voice::solo);
    engine.register_fn("set_output_bus", Voice::set_output_bus);

    // Actions
    engine.register_fn("apply", Voice::apply);
    engine.register_fn("run", Voice::run);
    engine.register_fn("trigger", voice_trigger);
    engine.register_fn("trigger", voice_trigger_no_params);
    engine.register_fn("stop", voice_stop);
    engine.register_fn("stop_all", voice_stop_all);
    engine.register_fn("note_on", voice_note_on);
    engine.register_fn("note_on", voice_note_on_int);
    engine.register_fn("note_on", voice_note_on_float);
    engine.register_fn("note_on", voice_note_on_int_vel);
    engine.register_fn("note_on", voice_note_on_float_vel);
    engine.register_fn("note_off", voice_note_off);
    engine.register_fn("control_change", voice_control_change);
}
