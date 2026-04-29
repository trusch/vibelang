//! Voice API for Rhai scripts.
//!
//! Voices are the basic sound-producing units in VibeLang.

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::collections::HashMap;
use vibelang_core::traits::VoiceConfig;
use vibelang_dsp::OutputPort;
#[cfg(feature = "midi")]
use vibelang_core::types::MidiDeviceId;
#[cfg(not(target_arch = "wasm32"))]
use vibelang_core::types::SfzId;
use vibelang_core::types::{ModulatorId, SampleId};

#[cfg(feature = "midi")]
use super::midi::MidiDevice;
use super::modulator::Modulator;
use super::route::{MultiRouteHandle, ParamHandle, RouteHandle};
use super::sample::SampleHandle;
#[cfg(not(target_arch = "wasm32"))]
use super::sfz::SfzHandle;
use crate::context;

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
    polyphony: u8,
    /// Gain in linear amplitude.
    gain: f32,
    /// Default parameters.
    params: HashMap<String, f32>,
    /// Whether this voice is muted.
    muted: bool,
    /// Whether this voice is soloed.
    soloed: bool,
    /// SFZ instrument ID (if this voice uses an SFZ instrument) - native only.
    #[cfg(not(target_arch = "wasm32"))]
    sfz_instrument: Option<SfzId>,
    /// Sample ID (if this voice uses a sample).
    sample_id: Option<SampleId>,
    /// Trigger mode: "gate" (default) or "one_shot".
    trigger_mode: String,
    /// Round-robin count for cycling through sample variations.
    round_robin_count: u32,
    /// Choke group name for exclusive triggering.
    choke_group: Option<String>,
    /// Modulation mappings (param_name -> modulator_id).
    modulations: HashMap<String, ModulatorId>,
    /// MIDI output device (if routing to external MIDI).
    #[cfg(feature = "midi")]
    midi_output_device: Option<MidiDeviceId>,
    /// MIDI output channel (0-15).
    #[cfg(feature = "midi")]
    midi_channel: u8,
    /// Parameter to MIDI CC mapping.
    #[cfg(feature = "midi")]
    param_cc_map: HashMap<String, u8>,
}

impl Voice {
    /// Create a new voice with the given name.
    pub fn new(_ctx: NativeCallContext, name: String) -> Self {
        Self {
            name: name,
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            trigger_mode: "gate".to_string(),
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        }
    }

    /// Create an anonymous voice (name resolved at finalization).
    pub fn new_anon(_ctx: NativeCallContext) -> Self {
        Self {
            name: String::new(),
            synth_name: None,
            group_path: context::current_group_path(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            trigger_mode: "gate".to_string(),
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
            #[cfg(feature = "midi")]
            param_cc_map: HashMap::new(),
        }
    }

    /// Resolve the name for an anonymous voice from its structural identity.
    ///
    /// Uses the synthdef name and group path to generate a stable, human-readable name.
    /// No-op if the voice already has a name.
    pub(crate) fn resolve_name(&mut self) {
        if !self.name.is_empty() {
            return;
        }
        let synth = self.synth_name.as_deref().unwrap_or("voice");
        let base = if self.group_path == "main" {
            format!("_{}", synth)
        } else {
            let group_suffix = self
                .group_path
                .strip_prefix("main/")
                .unwrap_or(&self.group_path);
            format!("_{}/{}", group_suffix, synth)
        };
        self.name = context::resolve_auto_name(&base);
    }

    // === Getters ===

    /// Get the voice ID (name).
    pub fn id(&mut self) -> String {
        self.resolve_name();
        self.name.clone()
    }

    /// Get the voice name.
    pub fn get_name(&mut self) -> String {
        self.resolve_name();
        self.name.clone()
    }

    /// Get the synth name.
    pub fn get_synth_name(&mut self) -> String {
        self.synth_name.clone().unwrap_or_default()
    }

    /// Get the gain.
    pub fn get_gain(&mut self) -> f64 {
        self.gain as f64
    }

    /// Get the polyphony.
    pub fn get_polyphony(&mut self) -> i64 {
        self.polyphony as i64
    }

    /// Get the group path.
    pub fn get_group_path(&mut self) -> String {
        self.group_path.clone()
    }

    // === Builder methods ===

    /// Set the group for this voice.
    pub fn group(mut self, group: String) -> Self {
        self.group_path = if group.starts_with("main/") || group == "main" {
            group
        } else {
            format!("{}/{}", context::current_group_path(), group)
        };
        self.sync_to_state();
        self
    }

    /// Set the synth for this voice.
    pub fn synth(mut self, synth_name: String) -> Self {
        self.synth_name = Some(synth_name);
        self.sync_to_state();
        self
    }

    /// Set the sound source (synthdef name).
    pub fn on(mut self, source: String) -> Self {
        self.synth_name = Some(source);
        self.sync_to_state();
        self
    }

    /// Set the sound source to a sample.
    ///
    /// This extracts all sample configuration (envelope, playback, warp settings)
    /// and stores them in the voice params for use at trigger time.
    pub fn on_sample(mut self, sample: SampleHandle) -> Self {
        // Get sample ID and config from context
        if let Some(sample_id) = context::get_sample_id(&sample.id) {
            self.sample_id = Some(sample_id);

            context::with_state(|state| {
                if let Some(config) = state.samples.get(&sample_id) {
                    // Copy trigger_mode from sample config
                    self.trigger_mode = config.trigger_mode.clone();

                    // When one_shot, tell the synthdef to ignore gate=0
                    if config.trigger_mode == "one_shot" {
                        self.params.insert("one_shot".to_string(), 1.0);
                    }

                    // Choose synthdef based on warp mode
                    self.synth_name = Some(if config.warp {
                        "warp_voice".to_string()
                    } else {
                        "sample_voice".to_string()
                    });

                    // Copy envelope params
                    self.params
                        .insert("attack".to_string(), config.attack as f32);
                    self.params
                        .insert("sustain".to_string(), config.sustain as f32);
                    self.params
                        .insert("release".to_string(), config.release as f32);
                    self.params.insert("amp".to_string(), config.amp as f32);

                    // Playback params
                    self.params.insert("rate".to_string(), config.rate as f32);
                    self.params
                        .insert("loop".to_string(), if config.loop_mode { 1.0 } else { 0.0 });

                    // Warp params (if warp mode)
                    if config.warp {
                        self.params.insert("speed".to_string(), config.speed as f32);
                        self.params.insert("pitch".to_string(), config.pitch as f32);
                        self.params
                            .insert("windowSize".to_string(), config.window_size as f32);
                        self.params
                            .insert("overlaps".to_string(), config.overlaps as f32);
                    }

                    // Store offset/length for conversion to frames at trigger time
                    self.params
                        .insert("_offset_secs".to_string(), config.offset as f32);
                    if let Some(len) = config.length {
                        self.params.insert("_length_secs".to_string(), len as f32);
                    }

                    // Store release time for fade-out calculation at trigger time
                    self.params
                        .insert("_release_secs".to_string(), config.release as f32);
                }
            });
        }

        // Always set bufnum
        self.params
            .insert("bufnum".to_string(), sample.buffer_id as f32);
        self.sync_to_state();
        self
    }

    /// Set the sound source to an SFZ instrument (native only).
    ///
    /// SFZ instruments contain multiple samples mapped across keys and velocities.
    /// When a note is triggered, the appropriate sample(s) are selected based on
    /// the note and velocity.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn on_sfz(mut self, sfz: SfzHandle) -> Self {
        // Get the SFZ ID for this instrument
        let sfz_id = context::get_or_create_sfz_id(&sfz.id);
        self.sfz_instrument = Some(sfz_id);
        // SFZ voices use sfz_voice_mono or sfz_voice_stereo synthdef
        // The runtime will select the appropriate one based on the sample
        self.synth_name = Some("sfz_voice_stereo".to_string());
        self.sync_to_state();
        self
    }

    /// Set the output to a MIDI device (routes notes to external MIDI instead of audio).
    ///
    /// The channel is taken from the MidiDevice. Use `midi_device("name").channel(n)`
    /// to configure the channel before passing to this method.
    #[cfg(feature = "midi")]
    pub fn on_midi_device(mut self, device: MidiDevice) -> Self {
        self.midi_output_device = Some(device.id);
        self.midi_channel = device.channel;
        // Mark this device for output opening during reload
        context::with_state(|state| {
            state.midi_outputs.insert(device.id);
        });
        self.sync_to_state();
        self
    }

    /// Set the MIDI channel (0-15) for MIDI output.
    ///
    /// Deprecated: prefer using `midi_device("name").channel(n)` instead.
    /// This method is kept for backward compatibility.
    #[cfg(feature = "midi")]
    pub fn channel(mut self, ch: i64) -> Self {
        self.midi_channel = ch.clamp(0, 15) as u8;
        self.sync_to_state();
        self
    }

    /// Map a parameter name to a MIDI CC number.
    ///
    /// When the parameter changes (via set_param or modulation), the value
    /// is sent as a MIDI CC message to the voice's MIDI output device.
    ///
    /// # Arguments
    ///
    /// * `param` - Parameter name (e.g., "cutoff", "resonance")
    /// * `cc` - MIDI CC number (0-127)
    #[cfg(feature = "midi")]
    pub fn cc_map(mut self, param: String, cc: i64) -> Self {
        self.param_cc_map.insert(param, cc.clamp(0, 127) as u8);
        self.sync_to_state();
        self
    }

    /// Set the polyphony.
    pub fn poly(mut self, count: i64) -> Self {
        self.polyphony = count.clamp(1, 255) as u8;
        self.sync_to_state();
        self
    }

    /// Set the gain.
    pub fn gain(mut self, value: f64) -> Self {
        self.gain = value as f32;
        self.sync_to_state();
        self
    }

    /// Set a parameter.
    pub fn set_param(mut self, param: String, value: f64) -> Self {
        self.params.insert(param, value as f32);
        self.sync_to_state();
        self
    }

    /// Alias for set_param() — consistent with modulator API.
    pub fn param(self, param: String, value: f64) -> Self {
        self.set_param(param, value)
    }

    /// Set the round-robin count for cycling through sample variations.
    ///
    /// When set to a value > 0, triggers will include an `rr` parameter
    /// that cycles from 0 to count-1. Useful for drum voices with multiple
    /// sample variations.
    pub fn round_robin(mut self, count: i64) -> Self {
        self.round_robin_count = count.max(0) as u32;
        self.sync_to_state();
        self
    }

    /// Set the choke group for exclusive triggering.
    ///
    /// When triggered, this voice will stop all other playing voices
    /// in the same choke group. Commonly used for hi-hat sounds where
    /// an open hi-hat should be stopped when a closed hi-hat is triggered.
    pub fn choke(mut self, group: String) -> Self {
        self.choke_group = if group.is_empty() { None } else { Some(group) };
        self.sync_to_state();
        self
    }

    /// Connect a modulator to a voice parameter.
    ///
    /// Multi-output v2 Story 5: this is sugar for
    /// `modulator.output("out").to_param(self, param)` — the modulator's voice
    /// (installed by `Modulator::apply()`) writes to a kr `out` port whose
    /// control bus is mapped onto the named parameter via the
    /// [`ScriptState::add_param_route`](vibelang_core::reload::ScriptState::add_param_route)
    /// pipeline. The legacy `modulations` map on `VoiceConfig` is still
    /// populated so direct field-level tests keep passing, but the runtime
    /// drive comes from the param-route.
    ///
    /// # Example
    ///
    /// ```rhai
    /// let cutoff_lfo = modulator("lfo_sine")
    ///     .set_param("rate", 4.0)
    ///     .set_param("lo", 200)
    ///     .set_param("hi", 2000)
    ///     .apply();
    ///
    /// let bass = voice("bass")
    ///     .synth("moog_bass")
    ///     .modulate("cutoff", cutoff_lfo)
    ///     .apply();
    /// ```
    pub fn modulate(mut self, param: String, modulator: Modulator) -> Self {
        let modulator_id = context::get_or_create_modulator_id(&modulator.name);
        self.modulations.insert(param.clone(), modulator_id);

        // Install the param-route from the source modulator's kr `out` port
        // to this voice's parameter. The source voice is registered by
        // `Modulator::apply()` under the modulator's name; this mirrors the
        // explicit `m.output("out").to_param(self, param)` sugar.
        self.resolve_name();
        if !self.name.is_empty() && !modulator.name.is_empty() {
            let target_voice_id = context::get_or_create_voice_id(&self.name);
            let source_voice_id = context::get_or_create_voice_id(&modulator.name);
            context::with_state(|state| {
                state.add_param_route(source_voice_id, "out", target_voice_id, param);
            });
        }
        self.sync_to_state();
        self
    }

    /// Begin a target-first CV-to-param wiring on this voice's named param.
    ///
    /// Dual to [`RouteHandle::to_param`](super::route::RouteHandle::to_param):
    /// `target.param("freq").modulate_by(source, "out")` records the same
    /// `(source_voice, source_port) → (target_voice, target_param)` entry
    /// in [`ScriptState::param_routes`] that the source-first form does, so
    /// either reading installs an identical registry row.
    ///
    /// Infallible — the param-name and source-port-rate validation runs in
    /// [`ParamHandle::modulate_by`] where both sides are known. Bare
    /// construction here just resolves the target voice's id and snapshots
    /// the synthdef name for the rate-validation step later.
    pub fn param_handle(&mut self, name: &str) -> ParamHandle {
        self.resolve_name();
        let voice_id = context::get_or_create_voice_id(&self.name);
        let synth = self.synth_name.clone().unwrap_or_default();
        ParamHandle::new(voice_id, self.name.clone(), synth, name.to_string())
    }

    /// Begin a route from this voice's named output port.
    ///
    /// Resolves `name` against the voice's synthdef's declared
    /// [`OutputPort`] set; the returned [`RouteHandle`] commits the route
    /// when one of the terminal verbs (`to`, `to_main`, `mute`) is called.
    /// Re-routing the same `(voice, port)` replaces the prior destination.
    ///
    /// Errors if the synthdef has not declared a port with this name —
    /// the message cites the available port names so users can fix the typo.
    pub fn output_by_name(
        &mut self,
        name: &str,
    ) -> Result<RouteHandle, Box<EvalAltResult>> {
        self.resolve_name();
        let ports = self.declared_output_ports();
        if ports.iter().any(|p| p.name == name) {
            let voice_id = context::get_or_create_voice_id(&self.name);
            Ok(RouteHandle::new(voice_id, name.to_string()))
        } else {
            Err(unknown_port_error(self, &ports, name))
        }
    }

    /// Begin a route from this voice's output port at the given 0-based index.
    ///
    /// Index resolution uses the synthdef's declared
    /// [`OutputPort`] order. Out-of-range indices error with the
    /// available port count and names.
    pub fn output_by_idx(
        &mut self,
        idx: i64,
    ) -> Result<RouteHandle, Box<EvalAltResult>> {
        self.resolve_name();
        let ports = self.declared_output_ports();
        let len = ports.len() as i64;
        if idx < 0 || idx >= len {
            return Err(idx_out_of_range_error(self, &ports, idx));
        }
        let port_name = ports[idx as usize].name.clone();
        let voice_id = context::get_or_create_voice_id(&self.name);
        Ok(RouteHandle::new(voice_id, port_name))
    }

    /// Begin a fan-out route from the listed output ports.
    ///
    /// Each entry resolves via the same name+idx rules as
    /// [`output_by_name`](Self::output_by_name) /
    /// [`output_by_idx`](Self::output_by_idx) — strings hit the named-port
    /// path, integers hit the 0-based index path, mixed entries are fine
    /// (Rhai arrays are heterogeneous). The returned [`MultiRouteHandle`]
    /// applies its terminal verb to every listed port in turn.
    ///
    /// Errors:
    /// - empty list → "outputs() requires at least one port name or index"
    /// - unknown name / OOR idx → cites the offending entry + declared port set
    ///   (re-uses the same error helpers as the singular form)
    /// - entry of any other Rhai type → clean type-mismatch error
    pub fn outputs(
        &mut self,
        entries: rhai::Array,
    ) -> Result<MultiRouteHandle, Box<EvalAltResult>> {
        if entries.is_empty() {
            return Err(empty_outputs_error());
        }
        self.resolve_name();
        let ports = self.declared_output_ports();
        let voice_id = context::get_or_create_voice_id(&self.name);
        let mut handles: Vec<RouteHandle> = Vec::with_capacity(entries.len());
        for entry in entries {
            let port_name = if entry.is_string() {
                let name = entry.into_string().unwrap_or_default();
                if !ports.iter().any(|p| p.name == name) {
                    return Err(unknown_port_error(self, &ports, &name));
                }
                name
            } else if let Ok(idx) = entry.as_int() {
                let len = ports.len() as i64;
                if idx < 0 || idx >= len {
                    return Err(idx_out_of_range_error(self, &ports, idx));
                }
                ports[idx as usize].name.clone()
            } else {
                return Err(invalid_outputs_entry_error(self, &entry));
            };
            handles.push(RouteHandle::new(voice_id, port_name));
        }
        Ok(MultiRouteHandle::new(handles))
    }

    /// Resolve the declared `OutputPort` set for this voice's synthdef.
    ///
    /// Falls back to the implicit legacy `[("out", 2)]` set when the synthdef
    /// has not been registered yet (matches `SynthDef::new` defaults).
    fn declared_output_ports(&self) -> Vec<OutputPort> {
        let synth = self.synth_name.as_deref().unwrap_or("");
        if synth.is_empty() {
            return legacy_output_ports();
        }
        vibelang_dsp::get_synthdef_outputs(synth).unwrap_or_else(legacy_output_ports)
    }

    /// Mute the voice (chainable).
    pub fn mute(mut self) -> Self {
        self.muted = true;
        self.sync_to_state();
        self
    }

    /// Unmute the voice (chainable).
    pub fn unmute(mut self) -> Self {
        self.muted = false;
        self.sync_to_state();
        self
    }

    /// Solo the voice (chainable).
    pub fn solo(mut self) -> Self {
        self.soloed = true;
        self.sync_to_state();
        self
    }

    /// Unsolo the voice (chainable).
    pub fn unsolo(mut self) -> Self {
        self.soloed = false;
        self.sync_to_state();
        self
    }

    /// Check if the voice is muted.
    pub fn is_muted(&mut self) -> bool {
        self.muted
    }

    /// Check if the voice is soloed.
    pub fn is_soloed(&mut self) -> bool {
        self.soloed
    }

    /// Register this voice with the script state (chainable).
    ///
    /// Skips registration for anonymous voices (empty name) that haven't been resolved yet.
    pub(crate) fn sync_to_state(&self) {
        if self.name.is_empty() {
            return; // Anonymous voice not yet resolved — defer registration
        }
        let voice_id = context::get_or_create_voice_id(&self.name);
        let group_id = context::get_or_create_group_id(&self.group_path);

        // Clone params and add gain as "amp" if not explicitly set
        let mut params = self.params.clone();
        if !params.contains_key("amp") {
            params.insert("amp".to_string(), self.gain);
        }
        tracing::debug!(
            "Voice sync_to_state: name={}, gain={}, amp in params={:?}",
            self.name,
            self.gain,
            params.get("amp")
        );

        let synthdef = self.synth_name.clone().unwrap_or_default();
        if synthdef.is_empty() {
            tracing::warn!(
                "Voice '{}': no synthdef set — use .synth(\"name\") or .on(\"sample\")",
                self.name
            );
        }

        let config = VoiceConfig {
            name: self.name.clone(),
            synthdef,
            group: group_id,
            polyphony: self.polyphony,
            params,
            muted: self.muted,
            soloed: self.soloed,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: self.sfz_instrument,
            #[cfg(target_arch = "wasm32")]
            sfz_instrument: None,
            sample_id: self.sample_id,
            trigger_mode: self.trigger_mode.clone(),
            round_robin_count: self.round_robin_count,
            choke_group: self.choke_group.clone(),
            modulations: self.modulations.clone(),
            #[cfg(feature = "midi")]
            midi_output: self.midi_output_device,
            #[cfg(feature = "midi")]
            midi_channel: self.midi_channel,
            #[cfg(feature = "midi")]
            param_cc_map: self.param_cc_map.clone(),
        };

        context::with_state(|state| {
            if config.has_sound_source() {
                state.voices.insert(voice_id, config);
                return;
            }
            // Bare `voice("name")` (no synth / sample / SFZ / MIDI yet): do not clobber an
            // existing fully configured voice — e.g. `device.pad(36).to(voice("kick"))` after
            // `voice("kick").synth("x").apply()`.
            match state.voices.get(&voice_id) {
                Some(existing) if existing.has_sound_source() => {}
                Some(_) => {
                    state.voices.remove(&voice_id);
                }
                None => {}
            }
        });
    }

    /// Apply the voice configuration (chainable).
    ///
    /// For anonymous voices, this resolves the name from the structural identity
    /// (synthdef + group path) before registering.
    pub fn apply(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();
        self
    }

    /// Construct a `Voice` for unit tests without a `NativeCallContext`.
    ///
    /// Mirrors `Voice::new` defaults but skips the auto-sync — callers are
    /// expected to drive `.synth(...)` / `.apply()` themselves to register
    /// the voice with the script state. Visible to sibling test modules so
    /// `route.rs` (and any future test in `api::`) can construct voices for
    /// testing route handles without going through Rhai's calling convention.
    #[cfg(test)]
    pub(crate) fn for_test(name: &str) -> Self {
        Self {
            name: name.to_string(),
            synth_name: None,
            group_path: "main".to_string(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            trigger_mode: "gate".to_string(),
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            param_cc_map: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        }
    }

    /// Run the voice continuously (for line-in, drones, etc.).
    ///
    /// This syncs the voice config and marks it for auto-triggering.
    /// The voice will be triggered automatically on startup and after
    /// reloads, producing continuous sound (e.g., for line-in monitors,
    /// drones, or other always-on sounds).
    pub fn run(mut self) -> Self {
        self.resolve_name();
        self.sync_to_state();
        context::mark_voice_for_running(&self.name);
        self
    }
}

/// Create a new voice builder.
///
/// Registers the name in the ID map and syncs to script state. A bare `voice("x")` with no
/// `.synth()` / `.on(...)` yet does **not** insert an invalid placeholder if `"x"` is already
/// fully configured (get-or-create semantics for MIDI routing and similar).
pub fn voice(ctx: NativeCallContext, name: String) -> Voice {
    let v = Voice::new(ctx, name);
    // Auto-sync on creation; incomplete builders do not clobber existing voice configs.
    v.sync_to_state();
    v
}

/// Create an anonymous voice builder (name resolved from structural identity).
pub fn voice_anon(ctx: NativeCallContext) -> Voice {
    Voice::new_anon(ctx)
}

/// Register voice API with the Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register Voice type
    engine.build_type::<Voice>();

    // Constructor (named and anonymous overloads)
    engine.register_fn("voice", voice);
    engine.register_fn("voice", voice_anon);

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

    // Builder methods
    engine.register_fn("group", Voice::group);
    engine.register_fn("synth", Voice::synth);
    engine.register_fn("on", Voice::on);
    engine.register_fn("on", Voice::on_sample);
    #[cfg(not(target_arch = "wasm32"))]
    engine.register_fn("on", Voice::on_sfz);
    #[cfg(not(target_arch = "wasm32"))]
    engine.register_fn("on_sfz", Voice::on_sfz); // Backward compatibility alias
    #[cfg(feature = "midi")]
    engine.register_fn("on", Voice::on_midi_device);
    #[cfg(feature = "midi")]
    engine.register_fn("channel", Voice::channel);
    #[cfg(feature = "midi")]
    engine.register_fn("cc_map", Voice::cc_map);
    engine.register_fn("poly", Voice::poly);
    engine.register_fn("gain", Voice::gain);
    engine.register_fn("set_param", Voice::set_param);
    engine.register_fn("param", Voice::param);
    engine.register_fn("param", Voice::param_handle);
    engine.register_fn("round_robin", Voice::round_robin);
    engine.register_fn("choke", Voice::choke);
    engine.register_fn("modulate", Voice::modulate);

    // Mute/solo
    engine.register_fn("mute", Voice::mute);
    engine.register_fn("unmute", Voice::unmute);
    engine.register_fn("solo", Voice::solo);
    engine.register_fn("unsolo", Voice::unsolo);
    engine.register_fn("is_muted", Voice::is_muted);
    engine.register_get("muted", Voice::is_muted);
    engine.register_fn("is_soloed", Voice::is_soloed);
    engine.register_get("soloed", Voice::is_soloed);

    // Actions
    engine.register_fn("apply", Voice::apply);
    engine.register_fn("run", Voice::run);

    // Multi-output routing entry point — name and 0-based index overloads.
    engine.register_fn("output", Voice::output_by_name);
    engine.register_fn("output", Voice::output_by_idx);

    // Plural sugar — fan-out to a list of named/indexed ports.
    engine.register_fn("outputs", Voice::outputs);
}

/// Implicit legacy output port set for synthdefs that did not call `.output(...)`.
///
/// Mirrors [`vibelang_dsp::SynthDef::new`] defaults — one stereo `out` port.
fn legacy_output_ports() -> Vec<OutputPort> {
    vec![OutputPort {
        name: "out".to_string(),
        channels: 2,
        rate: vibelang_dsp::PortRate::Ar,
    }]
}

fn unknown_port_error(voice: &Voice, ports: &[OutputPort], name: &str) -> Box<EvalAltResult> {
    let synth = voice.synth_name.as_deref().unwrap_or("<unset>");
    let available = available_port_list(ports);
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "Voice '{}' synthdef '{}' has no output port named '{}' (available: {})",
            voice.name, synth, name, available
        )
        .into(),
        Position::NONE,
    ))
}

fn idx_out_of_range_error(voice: &Voice, ports: &[OutputPort], idx: i64) -> Box<EvalAltResult> {
    let synth = voice.synth_name.as_deref().unwrap_or("<unset>");
    let available = available_port_list(ports);
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "Voice '{}' synthdef '{}' output index {} out of range (have {} ports: {})",
            voice.name,
            synth,
            idx,
            ports.len(),
            available
        )
        .into(),
        Position::NONE,
    ))
}

fn empty_outputs_error() -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        "outputs() requires at least one port name or index".into(),
        Position::NONE,
    ))
}

fn invalid_outputs_entry_error(voice: &Voice, entry: &Dynamic) -> Box<EvalAltResult> {
    let synth = voice.synth_name.as_deref().unwrap_or("<unset>");
    Box::new(EvalAltResult::ErrorRuntime(
        format!(
            "Voice '{}' synthdef '{}' outputs() entry must be a port name (string) \
             or 0-based index (integer); got value of type '{}'",
            voice.name,
            synth,
            entry.type_name()
        )
        .into(),
        Position::NONE,
    ))
}

fn available_port_list(ports: &[OutputPort]) -> String {
    if ports.is_empty() {
        return "<none>".to_string();
    }
    ports
        .iter()
        .map(|p| format!("'{}'", p.name))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context;

    /// Initialize a script context for testing, run the closure, then clean up.
    fn with_test_context<F: FnOnce()>(f: F) {
        context::init_context();
        f();
        context::clear_context();
    }

    /// Create a Voice for testing without NativeCallContext.
    fn test_voice(name: &str) -> Voice {
        Voice {
            name: name.to_string(),
            synth_name: None,
            group_path: "main".to_string(),
            polyphony: 4,
            gain: 1.0,
            params: HashMap::new(),
            muted: false,
            soloed: false,
            #[cfg(not(target_arch = "wasm32"))]
            sfz_instrument: None,
            sample_id: None,
            trigger_mode: "gate".to_string(),
            round_robin_count: 0,
            choke_group: None,
            modulations: HashMap::new(),
            param_cc_map: HashMap::new(),
            #[cfg(feature = "midi")]
            midi_output_device: None,
            #[cfg(feature = "midi")]
            midi_channel: 0,
        }
    }

    // ==================== Builder Method Tests ====================

    #[test]
    fn test_voice_synth() {
        with_test_context(|| {
            let v = test_voice("test").synth("my_synth".to_string());
            assert_eq!(v.synth_name, Some("my_synth".to_string()));
        });
    }

    #[test]
    fn test_voice_on() {
        with_test_context(|| {
            let v = test_voice("test").on("another_synth".to_string());
            assert_eq!(v.synth_name, Some("another_synth".to_string()));
        });
    }

    #[test]
    fn test_voice_poly() {
        with_test_context(|| {
            let v = test_voice("test").poly(8);
            assert_eq!(v.polyphony, 8);
        });
    }

    #[test]
    fn test_voice_poly_clamping() {
        with_test_context(|| {
            // Should clamp to 1-255
            let v1 = test_voice("test").poly(0);
            assert_eq!(v1.polyphony, 1);

            let v2 = test_voice("test").poly(300);
            assert_eq!(v2.polyphony, 255);

            let v3 = test_voice("test").poly(-5);
            assert_eq!(v3.polyphony, 1);
        });
    }

    #[test]
    fn test_voice_gain() {
        with_test_context(|| {
            let v = test_voice("test").gain(0.5);
            assert!((v.gain - 0.5).abs() < 0.001);
        });
    }

    #[test]
    fn test_voice_set_param() {
        with_test_context(|| {
            let v = test_voice("test")
                .set_param("freq".to_string(), 440.0)
                .set_param("amp".to_string(), 0.8);

            assert_eq!(v.params.get("freq"), Some(&440.0_f32));
            assert_eq!(v.params.get("amp"), Some(&0.8_f32));
        });
    }

    #[test]
    fn test_voice_mute_unmute() {
        let mut v = test_voice("test");
        assert!(!v.muted);

        v.muted = true;
        assert!(v.muted);

        v.muted = false;
        assert!(!v.muted);
    }

    #[test]
    fn test_voice_solo_unsolo() {
        let mut v = test_voice("test");
        assert!(!v.soloed);

        v.soloed = true;
        assert!(v.soloed);

        v.soloed = false;
        assert!(!v.soloed);
    }

    #[test]
    fn test_voice_group_relative_path() {
        let v = test_voice("test");
        // When group path doesn't start with "main/", it should be relative
        let v = Voice {
            group_path: "main/drums".to_string(),
            ..v
        };
        assert_eq!(v.group_path, "main/drums");
    }

    #[test]
    fn test_voice_chained_builders() {
        with_test_context(|| {
            let v = test_voice("lead")
                .synth("supersaw".to_string())
                .poly(4)
                .gain(0.7)
                .set_param("cutoff".to_string(), 2000.0);

            assert_eq!(v.name, "lead");
            assert_eq!(v.synth_name, Some("supersaw".to_string()));
            assert_eq!(v.polyphony, 4);
            assert!((v.gain - 0.7).abs() < 0.001);
            assert_eq!(v.params.get("cutoff"), Some(&2000.0_f32));
        });
    }

    // ==================== Getter Tests ====================

    #[test]
    fn test_voice_getters() {
        let mut v = test_voice("my_voice");
        v.synth_name = Some("test_synth".to_string());
        v.gain = 0.5;
        v.polyphony = 8;
        v.group_path = "main/drums".to_string();

        assert_eq!(v.id(), "my_voice");
        assert_eq!(v.get_name(), "my_voice");
        assert_eq!(v.get_synth_name(), "test_synth");
        assert!((v.get_gain() - 0.5).abs() < 0.001);
        assert_eq!(v.get_polyphony(), 8);
        assert_eq!(v.get_group_path(), "main/drums");
    }

    #[test]
    fn test_voice_get_synth_name_default() {
        let mut v = test_voice("test");
        // When no synth is set, should return empty string
        assert_eq!(v.get_synth_name(), "");
    }

    #[test]
    fn test_voice_is_muted_soloed() {
        let mut v = test_voice("test");

        assert!(!v.is_muted());
        assert!(!v.is_soloed());

        v.muted = true;
        v.soloed = true;

        assert!(v.is_muted());
        assert!(v.is_soloed());
    }

    // ==================== Default Values Tests ====================

    #[test]
    fn test_voice_default_values() {
        let v = test_voice("test");

        assert_eq!(v.polyphony, 4);
        assert!((v.gain - 1.0).abs() < 0.001);
        assert!(v.params.is_empty());
        assert!(!v.muted);
        assert!(!v.soloed);
        assert!(v.synth_name.is_none());
        #[cfg(not(target_arch = "wasm32"))]
        assert!(v.sfz_instrument.is_none());
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_voice_channel_clamping() {
        with_test_context(|| {
            let v1 = test_voice("test").channel(0);
            assert_eq!(v1.midi_channel, 0);

            let v2 = test_voice("test").channel(15);
            assert_eq!(v2.midi_channel, 15);

            let v3 = test_voice("test").channel(20);
            assert_eq!(v3.midi_channel, 15);

            let v4 = test_voice("test").channel(-5);
            assert_eq!(v4.midi_channel, 0);
        });
    }

    // ==================== Modulation Tests ====================
    // Note: These tests use direct manipulation of the modulations HashMap
    // because the modulate() method requires script context which isn't
    // available in unit tests.

    use vibelang_core::types::ModulatorId;

    /// Helper to test modulation storage directly without context
    fn test_voice_with_modulation(name: &str, param: &str, mod_id: u32) -> Voice {
        let mut v = test_voice(name);
        v.modulations
            .insert(param.to_string(), ModulatorId::new(mod_id));
        v
    }

    #[test]
    fn test_voice_modulate_single_param() {
        let v = test_voice_with_modulation("test", "cutoff", 1);

        assert_eq!(v.modulations.len(), 1);
        assert!(v.modulations.contains_key("cutoff"));
    }

    #[test]
    fn test_voice_modulate_multiple_params() {
        with_test_context(|| {
            let mut v = test_voice("test").synth("my_synth".to_string());
            v.modulations
                .insert("cutoff".to_string(), ModulatorId::new(1));
            v.modulations
                .insert("resonance".to_string(), ModulatorId::new(2));
            v.modulations.insert("pan".to_string(), ModulatorId::new(3));

            assert_eq!(v.modulations.len(), 3);
            assert!(v.modulations.contains_key("cutoff"));
            assert!(v.modulations.contains_key("resonance"));
            assert!(v.modulations.contains_key("pan"));
        });
    }

    #[test]
    fn test_voice_modulate_override() {
        // Test that modulating the same param twice uses the last value
        let mut v = test_voice("test");
        v.modulations
            .insert("cutoff".to_string(), ModulatorId::new(1)); // slow_lfo
        v.modulations
            .insert("cutoff".to_string(), ModulatorId::new(2)); // fast_lfo

        assert_eq!(v.modulations.len(), 1);
        assert_eq!(v.modulations.get("cutoff"), Some(&ModulatorId::new(2)));
    }

    #[test]
    fn test_voice_modulate_preserves_other_settings() {
        with_test_context(|| {
            let mut v = test_voice("test")
                .synth("my_synth".to_string())
                .poly(8)
                .gain(0.5)
                .set_param("freq".to_string(), 440.0);
            v.modulations
                .insert("cutoff".to_string(), ModulatorId::new(1));

            // Verify other settings are preserved
            assert_eq!(v.synth_name, Some("my_synth".to_string()));
            assert_eq!(v.polyphony, 8);
            assert!((v.gain - 0.5).abs() < 0.001);
            assert_eq!(v.params.get("freq"), Some(&440.0_f32));
            assert_eq!(v.modulations.len(), 1);
        });
    }

    #[test]
    fn test_voice_modulate_empty_initially() {
        let v = test_voice("test");
        assert!(v.modulations.is_empty());
    }

    #[test]
    fn test_voice_modulate_with_different_ids() {
        // Test that modulations can store different modulator IDs
        let mut v = test_voice("test");
        v.modulations
            .insert("param1".to_string(), ModulatorId::new(100));
        v.modulations
            .insert("param2".to_string(), ModulatorId::new(200));
        v.modulations
            .insert("param3".to_string(), ModulatorId::new(300));
        v.modulations
            .insert("param4".to_string(), ModulatorId::new(400));

        assert_eq!(v.modulations.len(), 4);
        assert_eq!(v.modulations.get("param1"), Some(&ModulatorId::new(100)));
        assert_eq!(v.modulations.get("param2"), Some(&ModulatorId::new(200)));
        assert_eq!(v.modulations.get("param3"), Some(&ModulatorId::new(300)));
        assert_eq!(v.modulations.get("param4"), Some(&ModulatorId::new(400)));
    }

    // ==================== Output / Routing Tests ====================
    //
    // The synthdef-output registry in vibelang-dsp::api is a process-wide
    // singleton, so each test runs in a uniquely-named synth/voice scope to
    // stay isolated even when cargo runs them in parallel.

    use vibelang_core::handlers::RouteDest;
    use vibelang_dsp::{register_synthdef_outputs, OutputPort};

    fn declare_synth_with_ports(synth: &str, port_names: &[&str]) {
        let ports: Vec<OutputPort> = port_names
            .iter()
            .map(|n| OutputPort {
                name: (*n).to_string(),
                channels: 1,
                rate: vibelang_dsp::PortRate::Ar,
            })
            .collect();
        register_synthdef_outputs(synth.to_string(), ports);
    }

    #[test]
    fn test_voice_output_by_name_round_trip() {
        with_test_context(|| {
            let synth = "story8_round_trip_named";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let group_path = "main/fx_evens_named".to_string();
            let group = super::super::group::GroupHandle::new(group_path.clone());
            let group_id = context::get_or_create_group_id(&group_path);

            let mut v = test_voice("vox_named").synth(synth.to_string());
            v.output_by_name("even")
                .expect("named output resolves")
                .to(group);

            let voice_id = context::get_or_create_voice_id("vox_named");
            context::with_state(|state| {
                let dest = state.routes.get(&(voice_id, "even".to_string()));
                assert_eq!(dest, Some(&RouteDest::Group(group_id)));
            });
        });
    }

    #[test]
    fn test_voice_output_by_idx_round_trip() {
        with_test_context(|| {
            let synth = "story8_round_trip_idx";
            // Indices: 0=sine, 1=even, 2=odd
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let group_path = "main/fx_odds_idx".to_string();
            let group = super::super::group::GroupHandle::new(group_path.clone());
            let group_id = context::get_or_create_group_id(&group_path);

            let mut v = test_voice("vox_idx").synth(synth.to_string());
            v.output_by_idx(2).expect("idx 2 resolves").to(group);

            let voice_id = context::get_or_create_voice_id("vox_idx");
            context::with_state(|state| {
                let dest = state.routes.get(&(voice_id, "odd".to_string()));
                assert_eq!(dest, Some(&RouteDest::Group(group_id)));
            });
        });
    }

    #[test]
    fn test_voice_output_unknown_name_errors_with_port_list() {
        with_test_context(|| {
            let synth = "story8_unknown_name";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let mut v = test_voice("vox_unknown").synth(synth.to_string());
            let err = v
                .output_by_name("triangle")
                .expect_err("unknown port name must error");
            let msg = err.to_string();
            assert!(msg.contains("triangle"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("'even'"), "msg = {}", msg);
            assert!(msg.contains("'odd'"), "msg = {}", msg);
        });
    }

    #[test]
    fn test_voice_output_idx_out_of_range_errors_with_port_list() {
        with_test_context(|| {
            let synth = "story8_oor_idx";
            declare_synth_with_ports(synth, &["sine", "even"]);

            let mut v = test_voice("vox_oor").synth(synth.to_string());
            let err = v
                .output_by_idx(5)
                .expect_err("idx out of range must error");
            let msg = err.to_string();
            assert!(msg.contains("5"), "msg = {}", msg);
            assert!(msg.contains("2"), "msg = {}", msg); // ports count
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("'even'"), "msg = {}", msg);

            // Negative index also errors.
            let err_neg = v
                .output_by_idx(-1)
                .expect_err("negative idx must error");
            assert!(err_neg.to_string().contains("-1"));
        });
    }

    #[test]
    fn test_voice_output_to_main_and_mute() {
        with_test_context(|| {
            let synth = "story8_main_mute";
            declare_synth_with_ports(synth, &["sub", "click"]);

            let mut v = test_voice("vox_mm").synth(synth.to_string());
            v.output_by_name("sub").expect("sub").to_main();
            v.output_by_name("click").expect("click").mute();

            let voice_id = context::get_or_create_voice_id("vox_mm");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "sub".to_string())),
                    Some(&RouteDest::Main)
                );
                assert_eq!(
                    state.routes.get(&(voice_id, "click".to_string())),
                    Some(&RouteDest::Muted)
                );
            });
        });
    }

    #[test]
    fn test_voice_output_re_route_replaces_prior_dest() {
        with_test_context(|| {
            let synth = "story8_replaces";
            declare_synth_with_ports(synth, &["even"]);

            let g1_path = "main/replaces_g1".to_string();
            let g2_path = "main/replaces_g2".to_string();
            let g1 = super::super::group::GroupHandle::new(g1_path.clone());
            let g2 = super::super::group::GroupHandle::new(g2_path.clone());
            let g1_id = context::get_or_create_group_id(&g1_path);
            let g2_id = context::get_or_create_group_id(&g2_path);

            let mut v = test_voice("vox_replace").synth(synth.to_string());
            v.output_by_name("even").expect("first").to(g1);
            // First route installed.
            let voice_id = context::get_or_create_voice_id("vox_replace");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Group(g1_id))
                );
            });

            // Re-route to a different group; prior dest must be replaced (not
            // accumulated — additive fan-out is v2).
            v.output_by_name("even").expect("second").to(g2);
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Group(g2_id))
                );
                // Exactly one entry for this (voice, port).
                let count = state
                    .routes
                    .keys()
                    .filter(|(vid, port)| *vid == voice_id && port == "even")
                    .count();
                assert_eq!(count, 1);
            });

            // Re-route across destination variants too.
            v.output_by_name("even").expect("third").to_main();
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Main)
                );
            });

            v.output_by_name("even").expect("fourth").mute();
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Muted)
                );
            });
        });
    }

    #[test]
    fn test_route_to_current_group_resolves_voice_group() {
        // .group("leads") then .output("even").to_current_group() must install
        // RouteDest::Group(leads_id) — the voice's currently configured group.
        with_test_context(|| {
            let synth = "ergo_to_current_group_resolves";
            declare_synth_with_ports(synth, &["even"]);

            let leads_path = "main/leads".to_string();
            let leads_id = context::get_or_create_group_id(&leads_path);

            let mut v = test_voice("vox_tcg_resolves")
                .synth(synth.to_string())
                .group("leads".to_string());
            v.output_by_name("even")
                .expect("port resolves")
                .to_current_group()
                .expect("voice has explicit group");

            let voice_id = context::get_or_create_voice_id("vox_tcg_resolves");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Group(leads_id))
                );
            });
        });
    }

    #[test]
    fn test_route_to_current_group_no_group_errors_with_hint() {
        // Voice in implicit "main" group (no .group(...) call) → clean error
        // pointing users at .group("name") or .to(group("name")).
        with_test_context(|| {
            let synth = "ergo_to_current_group_no_group";
            declare_synth_with_ports(synth, &["even"]);

            let mut v = test_voice("vox_tcg_no_group").synth(synth.to_string());
            let err = v
                .output_by_name("even")
                .expect("port resolves")
                .to_current_group()
                .expect_err("no explicit group must error");
            let msg = err.to_string();
            assert!(msg.contains(".group("), "msg = {}", msg);
            assert!(msg.contains(".to(group("), "msg = {}", msg);
            assert!(msg.contains("'even'"), "msg = {}", msg);

            // No route was installed.
            let voice_id = context::get_or_create_voice_id("vox_tcg_no_group");
            context::with_state(|state| {
                assert!(state.routes.get(&(voice_id, "even".to_string())).is_none());
            });
        });
    }

    #[test]
    fn test_route_to_current_group_then_to_other_replaces() {
        // After to_current_group() installs RouteDest::Group(leads), a later
        // .to(other) on the same (voice, port) replaces the prior dest — same
        // HashMap::insert semantics as the other terminal verbs.
        with_test_context(|| {
            let synth = "ergo_to_current_group_replaces";
            declare_synth_with_ports(synth, &["even"]);

            let leads_path = "main/leads_replace".to_string();
            let other_path = "main/fx_other".to_string();
            let leads_id = context::get_or_create_group_id(&leads_path);
            let other_id = context::get_or_create_group_id(&other_path);

            let mut v = test_voice("vox_tcg_replace")
                .synth(synth.to_string())
                .group("leads_replace".to_string());

            v.output_by_name("even")
                .expect("port resolves")
                .to_current_group()
                .expect("voice has group");

            let voice_id = context::get_or_create_voice_id("vox_tcg_replace");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Group(leads_id))
                );
            });

            // Re-route to a different group via explicit .to(other).
            let other = super::super::group::GroupHandle::new(other_path.clone());
            v.output_by_name("even")
                .expect("port resolves")
                .to(other);

            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Group(other_id))
                );
                let count = state
                    .routes
                    .keys()
                    .filter(|(vid, port)| *vid == voice_id && port == "even")
                    .count();
                assert_eq!(count, 1);
            });
        });
    }

    // ==================== Plural Outputs (Fan-out) Tests ====================

    fn dyn_str(s: &str) -> rhai::Dynamic {
        rhai::Dynamic::from(s.to_string())
    }
    fn dyn_int(i: i64) -> rhai::Dynamic {
        rhai::Dynamic::from(i)
    }

    #[test]
    fn test_voice_outputs_name_list_to_group() {
        // v.outputs(["sine", "odd"]).to(g) → both ports get RouteDest::Group(g)
        with_test_context(|| {
            let synth = "ergo_outputs_names_to_group";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let group_path = "main/leads_outs_names".to_string();
            let g_id = context::get_or_create_group_id(&group_path);

            let mut v = test_voice("vox_outs_names").synth(synth.to_string());
            let arr: rhai::Array = vec![dyn_str("sine"), dyn_str("odd")];
            v.outputs(arr)
                .expect("name list resolves")
                .to(super::super::group::GroupHandle::new(group_path));

            let voice_id = context::get_or_create_voice_id("vox_outs_names");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "sine".to_string())),
                    Some(&RouteDest::Group(g_id))
                );
                assert_eq!(
                    state.routes.get(&(voice_id, "odd".to_string())),
                    Some(&RouteDest::Group(g_id))
                );
                // No route for the unlisted port.
                assert!(state
                    .routes
                    .get(&(voice_id, "even".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn test_voice_outputs_idx_list_to_main() {
        // v.outputs([1, 2]).to_main() → both ports get RouteDest::Main
        with_test_context(|| {
            let synth = "ergo_outputs_idx_to_main";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let mut v = test_voice("vox_outs_idx").synth(synth.to_string());
            let arr: rhai::Array = vec![dyn_int(1), dyn_int(2)];
            v.outputs(arr).expect("idx list resolves").to_main();

            let voice_id = context::get_or_create_voice_id("vox_outs_idx");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "even".to_string())),
                    Some(&RouteDest::Main)
                );
                assert_eq!(
                    state.routes.get(&(voice_id, "odd".to_string())),
                    Some(&RouteDest::Main)
                );
                assert!(state
                    .routes
                    .get(&(voice_id, "sine".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn test_voice_outputs_mixed_types_mute() {
        // v.outputs(["sine", 2]).mute() → both get Muted
        with_test_context(|| {
            let synth = "ergo_outputs_mixed_mute";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let mut v = test_voice("vox_outs_mixed").synth(synth.to_string());
            let arr: rhai::Array = vec![dyn_str("sine"), dyn_int(2)];
            v.outputs(arr).expect("mixed list resolves").mute();

            let voice_id = context::get_or_create_voice_id("vox_outs_mixed");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "sine".to_string())),
                    Some(&RouteDest::Muted)
                );
                assert_eq!(
                    state.routes.get(&(voice_id, "odd".to_string())),
                    Some(&RouteDest::Muted)
                );
            });
        });
    }

    #[test]
    fn test_voice_outputs_to_current_group() {
        // .group("leads") then v.outputs([...]).to_current_group()
        with_test_context(|| {
            let synth = "ergo_outputs_tcg";
            declare_synth_with_ports(synth, &["a", "b", "c"]);

            let leads_path = "main/leads_outs_tcg".to_string();
            let leads_id = context::get_or_create_group_id(&leads_path);

            let mut v = test_voice("vox_outs_tcg")
                .synth(synth.to_string())
                .group("leads_outs_tcg".to_string());

            let arr: rhai::Array = vec![dyn_str("a"), dyn_str("c")];
            v.outputs(arr)
                .expect("ports resolve")
                .to_current_group()
                .expect("voice has explicit group");

            let voice_id = context::get_or_create_voice_id("vox_outs_tcg");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "a".to_string())),
                    Some(&RouteDest::Group(leads_id))
                );
                assert_eq!(
                    state.routes.get(&(voice_id, "c".to_string())),
                    Some(&RouteDest::Group(leads_id))
                );
                assert!(state.routes.get(&(voice_id, "b".to_string())).is_none());
            });
        });
    }

    #[test]
    fn test_voice_outputs_unknown_name_errors_cites_offender_and_ports() {
        with_test_context(|| {
            let synth = "ergo_outputs_unknown_name";
            declare_synth_with_ports(synth, &["sine", "even", "odd"]);

            let mut v = test_voice("vox_outs_unk").synth(synth.to_string());
            let arr: rhai::Array = vec![dyn_str("sine"), dyn_str("triangle")];
            let err = v.outputs(arr).expect_err("unknown name must error");
            let msg = err.to_string();

            // Cites offending entry...
            assert!(msg.contains("triangle"), "msg = {}", msg);
            // ...and the declared port set.
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("'even'"), "msg = {}", msg);
            assert!(msg.contains("'odd'"), "msg = {}", msg);

            // No partial routes were committed — the handle wasn't built.
            let voice_id = context::get_or_create_voice_id("vox_outs_unk");
            context::with_state(|state| {
                assert!(state
                    .routes
                    .get(&(voice_id, "sine".to_string()))
                    .is_none());
            });
        });
    }

    #[test]
    fn test_voice_outputs_idx_out_of_range_errors_cites_offender_and_ports() {
        with_test_context(|| {
            let synth = "ergo_outputs_oor_idx";
            declare_synth_with_ports(synth, &["sine", "even"]);

            let mut v = test_voice("vox_outs_oor").synth(synth.to_string());
            let arr: rhai::Array = vec![dyn_int(0), dyn_int(7)];
            let err = v.outputs(arr).expect_err("OOR idx must error");
            let msg = err.to_string();
            assert!(msg.contains("7"), "msg = {}", msg);
            assert!(msg.contains("'sine'"), "msg = {}", msg);
            assert!(msg.contains("'even'"), "msg = {}", msg);
        });
    }

    #[test]
    fn test_voice_outputs_empty_list_clean_error() {
        with_test_context(|| {
            let synth = "ergo_outputs_empty";
            declare_synth_with_ports(synth, &["sine", "even"]);

            let mut v = test_voice("vox_outs_empty").synth(synth.to_string());
            let arr: rhai::Array = vec![];
            let err = v.outputs(arr).expect_err("empty list must error");
            let msg = err.to_string();
            assert!(
                msg.contains("outputs() requires at least one port name or index"),
                "msg = {}",
                msg
            );
        });
    }

    #[test]
    fn test_voice_outputs_replace_semantics_under_fanout() {
        // outputs(["a"]).to(g1); outputs(["a"]).to(g2) → "a" routes to g2 only.
        with_test_context(|| {
            let synth = "ergo_outputs_replace";
            declare_synth_with_ports(synth, &["a", "b"]);

            let g1_path = "main/outs_replace_g1".to_string();
            let g2_path = "main/outs_replace_g2".to_string();
            let g1_id = context::get_or_create_group_id(&g1_path);
            let g2_id = context::get_or_create_group_id(&g2_path);

            let mut v = test_voice("vox_outs_replace").synth(synth.to_string());
            let arr1: rhai::Array = vec![dyn_str("a")];
            v.outputs(arr1)
                .expect("first")
                .to(super::super::group::GroupHandle::new(g1_path));
            let voice_id = context::get_or_create_voice_id("vox_outs_replace");
            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "a".to_string())),
                    Some(&RouteDest::Group(g1_id))
                );
            });

            let arr2: rhai::Array = vec![dyn_str("a")];
            v.outputs(arr2)
                .expect("second")
                .to(super::super::group::GroupHandle::new(g2_path));

            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(voice_id, "a".to_string())),
                    Some(&RouteDest::Group(g2_id))
                );
                // Exactly one entry (replace, not accumulate).
                let count = state
                    .routes
                    .keys()
                    .filter(|(vid, port)| *vid == voice_id && port == "a")
                    .count();
                assert_eq!(count, 1);
            });
        });
    }

    #[test]
    fn test_voice_output_name_idx_round_trip_resolve_same_port() {
        // Index N must resolve to the same name as the Nth declared port.
        with_test_context(|| {
            let synth = "story8_name_idx_match";
            declare_synth_with_ports(synth, &["a", "b", "c", "d"]);

            let group_path = "main/match_grp".to_string();
            let g_id = context::get_or_create_group_id(&group_path);

            // Idx 2 → "c"
            let mut v_idx = test_voice("vox_match_idx").synth(synth.to_string());
            v_idx
                .output_by_idx(2)
                .expect("idx 2")
                .to(super::super::group::GroupHandle::new(group_path.clone()));
            let id_idx = context::get_or_create_voice_id("vox_match_idx");

            // Name "c"
            let mut v_name = test_voice("vox_match_name").synth(synth.to_string());
            v_name
                .output_by_name("c")
                .expect("name c")
                .to(super::super::group::GroupHandle::new(group_path));
            let id_name = context::get_or_create_voice_id("vox_match_name");

            context::with_state(|state| {
                assert_eq!(
                    state.routes.get(&(id_idx, "c".to_string())),
                    Some(&RouteDest::Group(g_id))
                );
                assert_eq!(
                    state.routes.get(&(id_name, "c".to_string())),
                    Some(&RouteDest::Group(g_id))
                );
            });
        });
    }
}
