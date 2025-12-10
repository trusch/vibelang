//! High-level DSP helper functions.
//!
//! This module provides convenience functions for common DSP operations
//! like envelopes, mixing, and bus I/O.

use super::errors::*;
use super::graph::*;
use super::rhainodes::NodeRef;
use rhai::{Array, Dynamic};

/// Convert a Rhai value to an Input (either parameter NodeRef or constant).
pub fn dynamic_to_input(value: &Dynamic) -> Result<Input> {
    if let Some(node) = value.clone().try_cast::<NodeRef>() {
        Ok(node.to_input())
    } else if let Some(f) = value.clone().try_cast::<f64>() {
        let _ = with_builder(|builder| {
            builder.add_constant(f as f32);
        });
        Ok(Input::Constant(f as f32))
    } else if let Some(i) = value.clone().try_cast::<i64>() {
        let _ = with_builder(|builder| {
            builder.add_constant(i as f32);
        });
        Ok(Input::Constant(i as f32))
    } else if let Some(i) = value.clone().try_cast::<i32>() {
        let _ = with_builder(|builder| {
            builder.add_constant(i as f32);
        });
        Ok(Input::Constant(i as f32))
    } else {
        Err(SynthDefError::ValidationError(format!(
            "Expected number or signal, got {}",
            value.type_name()
        )))
    }
}

/// Mix an array of NodeRefs into a single signal.
pub fn mix(signals: Array) -> Result<NodeRef> {
    if signals.is_empty() {
        return Err(SynthDefError::InvalidBodyReturn);
    }

    let mut result = signals[0]
        .clone()
        .try_cast::<NodeRef>()
        .ok_or(SynthDefError::InvalidBodyReturn)?;

    for sig_dyn in signals.iter().skip(1) {
        let sig = sig_dyn
            .clone()
            .try_cast::<NodeRef>()
            .ok_or(SynthDefError::InvalidBodyReturn)?;
        result = result.add(sig)?;
    }

    Ok(result)
}

/// Expose individual outputs from a multi-output UGen as separate NodeRefs.
pub fn channels(signal: NodeRef, count: i64) -> Result<Array> {
    if count <= 0 {
        return Err(SynthDefError::ValidationError(
            "channels() count must be positive".to_string(),
        ));
    }

    if signal.0 >= 0x80000000 {
        return Err(SynthDefError::ValidationError(
            "channels() cannot operate on parameter references".to_string(),
        ));
    }

    let mut outputs = Array::new();
    for ch in 0..count {
        outputs.push(Dynamic::from(NodeRef::new_with_output(
            signal.id(),
            ch as u32,
        )));
    }
    Ok(outputs)
}

/// Get a specific output from a multi-output UGen.
pub fn channel(signal: NodeRef, index: i64) -> Result<NodeRef> {
    if index < 0 {
        return Err(SynthDefError::ValidationError(
            "channel() index must be non-negative".to_string(),
        ));
    }

    if signal.0 >= 0x80000000 {
        return Err(SynthDefError::ValidationError(
            "channel() cannot operate on parameter references".to_string(),
        ));
    }

    Ok(NodeRef::new_with_output(signal.id(), index as u32))
}

/// Generate a detune spread array for supersaw/unison effects.
pub fn detune_spread(voices: i64, amount: f64) -> Result<Array> {
    let mut result = Array::new();
    let half_voices = (voices as f64 - 1.0) / 2.0;

    for i in 0..voices {
        let offset = (i as f64 - half_voices) / half_voices;
        let detune = 1.0 + (offset * amount);
        result.push(Dynamic::from(detune));
    }

    Ok(result)
}

/// Simple envelope generator helper.
pub fn env_gen(gate: NodeRef, done_action: i64) -> Result<NodeRef> {
    with_builder(|builder| {
        builder.add_constant(1.0);
        builder.add_constant(0.0);
        builder.add_constant(done_action as f32);

        let inputs = vec![
            gate.to_input(),
            Input::Constant(1.0),
            Input::Constant(0.0),
            Input::Constant(1.0),
            Input::Constant(done_action as f32),
        ];
        builder.add_node("EnvGen".to_string(), Rate::Audio, inputs, 1, 0)
    })
}

/// Convert decibels to amplitude.
pub fn db_to_amp(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Convert amplitude to decibels.
pub fn amp_to_db(amp: f64) -> f64 {
    20.0 * amp.log10()
}

/// Duplicate a signal N times into an array.
pub fn dup(signal: NodeRef, count: i64) -> Result<Array> {
    let mut result = Array::new();
    for _ in 0..count {
        result.push(Dynamic::from(signal));
    }
    Ok(result)
}

/// Envelope specification (like SuperCollider's Env class).
#[derive(Clone, Debug)]
pub struct Env {
    pub levels: Vec<f32>,
    pub times: Vec<f32>,
    pub curves: Vec<f32>,
    pub release_node: i32,
}

impl Env {
    /// Create a new envelope specification.
    pub fn new(levels: Array, times: Array, curve: f64) -> Result<Self> {
        let levels_vec: Vec<f32> = levels
            .iter()
            .map(|v| v.clone().try_cast::<f64>().unwrap_or(0.0) as f32)
            .collect();
        let times_vec: Vec<f32> = times
            .iter()
            .map(|v| v.clone().try_cast::<f64>().unwrap_or(0.0) as f32)
            .collect();

        let curve_val = curve as f32;
        let curves_vec = vec![curve_val; times_vec.len()];

        Ok(Env {
            levels: levels_vec,
            times: times_vec,
            curves: curves_vec,
            release_node: -1,
        })
    }

    /// Create a percussive envelope (attack, release).
    pub fn perc(attack: f64, release: f64) -> Self {
        Env {
            levels: vec![0.0, 1.0, 0.0],
            times: vec![attack as f32, release as f32],
            curves: vec![1.0, 1.0],
            release_node: -1,
        }
    }

    /// Create an ADSR envelope.
    pub fn adsr(attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        Env {
            levels: vec![0.0, 1.0, sustain as f32, sustain as f32, 0.0],
            times: vec![attack as f32, decay as f32, 0.0, release as f32],
            curves: vec![1.0, 1.0, 1.0, 1.0],
            release_node: 3,
        }
    }

    /// Create an ASR envelope.
    pub fn asr(attack: f64, sustain: f64, release: f64) -> Self {
        Env {
            levels: vec![0.0, sustain as f32, 0.0],
            times: vec![attack as f32, release as f32],
            curves: vec![1.0, 1.0],
            release_node: 1,
        }
    }

    /// Create a triangle envelope.
    pub fn triangle(duration: f64) -> Self {
        Env {
            levels: vec![0.0, 1.0, 0.0],
            times: vec![(duration / 2.0) as f32, (duration / 2.0) as f32],
            curves: vec![1.0, 1.0],
            release_node: -1,
        }
    }
}

/// EnvGen with Env and f64 parameters.
pub fn env_gen_with_env(
    env: Env,
    gate: NodeRef,
    level_scale: f64,
    level_bias: f64,
    time_scale: f64,
    done_action: f64,
) -> Result<NodeRef> {
    with_builder(|builder| {
        builder.add_constant(level_scale as f32);
        builder.add_constant(level_bias as f32);
        builder.add_constant(time_scale as f32);
        builder.add_constant(done_action as f32);
    })?;

    env_gen_with_env_impl(
        env,
        gate,
        Input::Constant(level_scale as f32),
        Input::Constant(level_bias as f32),
        Input::Constant(time_scale as f32),
        Input::Constant(done_action as f32),
    )
}

/// EnvGen with NodeRef parameters.
pub fn env_gen_with_env_n(
    env: Env,
    gate: NodeRef,
    level_scale: NodeRef,
    level_bias: NodeRef,
    time_scale: NodeRef,
    done_action: NodeRef,
) -> Result<NodeRef> {
    env_gen_with_env_impl(
        env,
        gate,
        level_scale.to_input(),
        level_bias.to_input(),
        time_scale.to_input(),
        done_action.to_input(),
    )
}

fn env_gen_with_env_impl(
    env: Env,
    gate: NodeRef,
    level_scale: Input,
    level_bias: Input,
    time_scale: Input,
    done_action: Input,
) -> Result<NodeRef> {
    with_builder(|builder| {
        let num_levels = env.levels.len();
        let num_stages = (num_levels - 1) as f32;
        let init_level = env.levels[0];
        let release_node = env.release_node as f32;
        let loop_node = -1.0f32;

        builder.add_constant(init_level);
        builder.add_constant(num_stages);
        builder.add_constant(release_node);
        builder.add_constant(loop_node);

        for i in 0..(num_levels - 1) {
            builder.add_constant(env.levels[i + 1]);
            builder.add_constant(env.times[i]);
            let curve_val = env.curves[i];
            let shape = if curve_val == 1.0 { 1.0 } else if curve_val == 2.0 { 2.0 } else { 5.0 };
            builder.add_constant(shape);
            builder.add_constant(curve_val);
        }

        let mut inputs = vec![
            gate.to_input(),
            level_scale,
            level_bias,
            time_scale,
            done_action,
        ];

        inputs.push(Input::Constant(init_level));
        inputs.push(Input::Constant(num_stages));
        inputs.push(Input::Constant(release_node));
        inputs.push(Input::Constant(loop_node));

        for i in 0..(num_levels - 1) {
            inputs.push(Input::Constant(env.levels[i + 1]));
            inputs.push(Input::Constant(env.times[i]));
            let curve_val = env.curves[i];
            let shape = if curve_val == 1.0 { 1.0 } else if curve_val == 2.0 { 2.0 } else { 5.0 };
            inputs.push(Input::Constant(shape));
            inputs.push(Input::Constant(curve_val));
        }

        builder.add_node("EnvGen".to_string(), Rate::Audio, inputs, 1, 0)
    })
}

/// Enum to hold either a constant f64 or a NodeRef for EnvGen parameters.
#[derive(Clone, Debug)]
pub enum EnvGenParam {
    Constant(f64),
    Node(NodeRef),
}

impl EnvGenParam {
    fn to_input(&self) -> Input {
        match self {
            EnvGenParam::Constant(v) => Input::Constant(*v as f32),
            EnvGenParam::Node(n) => n.to_input(),
        }
    }
}

/// Builder pattern for EnvGen.
#[derive(Clone, Debug)]
pub struct EnvGenBuilder {
    env: Env,
    gate: NodeRef,
    level_scale: Option<EnvGenParam>,
    level_bias: Option<EnvGenParam>,
    time_scale: Option<EnvGenParam>,
    done_action: Option<EnvGenParam>,
}

impl EnvGenBuilder {
    pub fn new(env: Env, gate: NodeRef) -> Self {
        EnvGenBuilder {
            env,
            gate,
            level_scale: None,
            level_bias: None,
            time_scale: None,
            done_action: None,
        }
    }

    pub fn with_level_scale(mut self, level_scale: f64) -> Self {
        self.level_scale = Some(EnvGenParam::Constant(level_scale));
        self
    }

    pub fn with_level_scale_n(mut self, level_scale: NodeRef) -> Self {
        self.level_scale = Some(EnvGenParam::Node(level_scale));
        self
    }

    pub fn with_level_bias(mut self, level_bias: f64) -> Self {
        self.level_bias = Some(EnvGenParam::Constant(level_bias));
        self
    }

    pub fn with_level_bias_n(mut self, level_bias: NodeRef) -> Self {
        self.level_bias = Some(EnvGenParam::Node(level_bias));
        self
    }

    pub fn with_time_scale(mut self, time_scale: f64) -> Self {
        self.time_scale = Some(EnvGenParam::Constant(time_scale));
        self
    }

    pub fn with_time_scale_n(mut self, time_scale: NodeRef) -> Self {
        self.time_scale = Some(EnvGenParam::Node(time_scale));
        self
    }

    pub fn with_done_action(mut self, done_action: f64) -> Self {
        self.done_action = Some(EnvGenParam::Constant(done_action));
        self
    }

    pub fn with_done_action_n(mut self, done_action: NodeRef) -> Self {
        self.done_action = Some(EnvGenParam::Node(done_action));
        self
    }

    pub fn build(self) -> Result<NodeRef> {
        let level_scale = self.level_scale.unwrap_or(EnvGenParam::Constant(1.0));
        let level_bias = self.level_bias.unwrap_or(EnvGenParam::Constant(0.0));
        let time_scale = self.time_scale.unwrap_or(EnvGenParam::Constant(1.0));
        let done_action = self.done_action.unwrap_or(EnvGenParam::Constant(0.0));

        env_gen_with_env_impl(
            self.env,
            self.gate,
            level_scale.to_input(),
            level_bias.to_input(),
            time_scale.to_input(),
            done_action.to_input(),
        )
    }
}

/// Parse a duration value from either a f64 (seconds) or a humantime string.
fn parse_duration(value: rhai::Dynamic) -> Result<f64> {
    if let Some(f) = value.clone().try_cast::<f64>() {
        Ok(f)
    } else if let Some(i) = value.clone().try_cast::<i64>() {
        Ok(i as f64)
    } else if let Some(s) = value.clone().try_cast::<rhai::ImmutableString>() {
        humantime::parse_duration(s.as_str())
            .map(|d| d.as_secs_f64())
            .map_err(|e| SynthDefError::ValidationError(format!("Invalid duration '{}': {}", s, e)))
    } else {
        Err(SynthDefError::ValidationError(format!(
            "Expected number or duration string, got {}",
            value.type_name()
        )))
    }
}

/// Fluent builder for envelope generators.
///
/// This builder provides a convenient API for creating envelope generators
/// with smart selection of the underlying envelope type (perc, asr, adsr).
///
/// # Example
/// ```ignore
/// // Simple percussive envelope
/// let env = envelope()
///     .attack("5ms")
///     .release("50ms")
///     .gate(gate)
///     .cleanup_on_finish()
///     .build();
///
/// // ADSR envelope
/// let env = envelope()
///     .adsr("10ms", "100ms", 0.7, "200ms")
///     .gate(gate)
///     .build();
/// ```
#[derive(Clone, Debug)]
pub struct EnvelopeBuilder {
    // Envelope parameters
    attack: Option<f64>,
    decay: Option<f64>,
    sustain: Option<f64>,
    release: Option<f64>,

    // Gate and done action
    gate: Option<NodeRef>,
    done_action: Option<EnvGenParam>,

    // Optional modifiers (from EnvGenBuilder)
    level_scale: Option<EnvGenParam>,
    level_bias: Option<EnvGenParam>,
    time_scale: Option<EnvGenParam>,
}

impl EnvelopeBuilder {
    /// Create a new envelope builder with default values.
    pub fn new() -> Self {
        EnvelopeBuilder {
            attack: None,
            decay: None,
            sustain: None,
            release: None,
            gate: None,
            done_action: None,
            level_scale: None,
            level_bias: None,
            time_scale: None,
        }
    }

    /// Set the attack time.
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings like "5ms", "1s".
    pub fn attack(mut self, value: rhai::Dynamic) -> Self {
        if let Ok(t) = parse_duration(value) {
            self.attack = Some(t);
        }
        self
    }

    /// Set the attack time from f64 (seconds).
    pub fn attack_f(mut self, seconds: f64) -> Self {
        self.attack = Some(seconds);
        self
    }

    /// Set the decay time.
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings like "5ms", "1s".
    pub fn decay(mut self, value: rhai::Dynamic) -> Self {
        if let Ok(t) = parse_duration(value) {
            self.decay = Some(t);
        }
        self
    }

    /// Set the decay time from f64 (seconds).
    pub fn decay_f(mut self, seconds: f64) -> Self {
        self.decay = Some(seconds);
        self
    }

    /// Set the sustain level (0.0 to 1.0).
    pub fn sustain(mut self, level: f64) -> Self {
        self.sustain = Some(level.clamp(0.0, 1.0));
        self
    }

    /// Set the release time.
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings like "5ms", "1s".
    pub fn release(mut self, value: rhai::Dynamic) -> Self {
        if let Ok(t) = parse_duration(value) {
            self.release = Some(t);
        }
        self
    }

    /// Set the release time from f64 (seconds).
    pub fn release_f(mut self, seconds: f64) -> Self {
        self.release = Some(seconds);
        self
    }

    /// Set the gate signal (NodeRef).
    pub fn gate(mut self, gate: NodeRef) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Set done_action to 2 (free the synth when envelope completes).
    pub fn cleanup_on_finish(mut self) -> Self {
        self.done_action = Some(EnvGenParam::Constant(2.0));
        self
    }

    /// Set a custom done_action value.
    pub fn done_action(mut self, action: f64) -> Self {
        self.done_action = Some(EnvGenParam::Constant(action));
        self
    }

    /// Set done_action from a NodeRef.
    pub fn done_action_n(mut self, action: NodeRef) -> Self {
        self.done_action = Some(EnvGenParam::Node(action));
        self
    }

    /// Set the level scale (amplitude multiplier).
    pub fn level_scale(mut self, scale: f64) -> Self {
        self.level_scale = Some(EnvGenParam::Constant(scale));
        self
    }

    /// Set the level scale from a NodeRef.
    pub fn level_scale_n(mut self, scale: NodeRef) -> Self {
        self.level_scale = Some(EnvGenParam::Node(scale));
        self
    }

    /// Set the level bias (DC offset).
    pub fn level_bias(mut self, bias: f64) -> Self {
        self.level_bias = Some(EnvGenParam::Constant(bias));
        self
    }

    /// Set the level bias from a NodeRef.
    pub fn level_bias_n(mut self, bias: NodeRef) -> Self {
        self.level_bias = Some(EnvGenParam::Node(bias));
        self
    }

    /// Set the time scale (time multiplier).
    pub fn time_scale(mut self, scale: f64) -> Self {
        self.time_scale = Some(EnvGenParam::Constant(scale));
        self
    }

    /// Set the time scale from a NodeRef.
    pub fn time_scale_n(mut self, scale: NodeRef) -> Self {
        self.time_scale = Some(EnvGenParam::Node(scale));
        self
    }

    /// Configure a percussive envelope (attack → release, no sustain).
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings.
    pub fn perc(mut self, attack: rhai::Dynamic, release: rhai::Dynamic) -> Self {
        if let Ok(a) = parse_duration(attack) {
            self.attack = Some(a);
        }
        if let Ok(r) = parse_duration(release) {
            self.release = Some(r);
        }
        // Clear decay and sustain to ensure perc envelope
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure a percussive envelope with f64 values.
    pub fn perc_f(mut self, attack: f64, release: f64) -> Self {
        self.attack = Some(attack);
        self.release = Some(release);
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure an ASR envelope (attack → sustain → release).
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings for times.
    pub fn asr(
        mut self,
        attack: rhai::Dynamic,
        sustain: f64,
        release: rhai::Dynamic,
    ) -> Self {
        if let Ok(a) = parse_duration(attack) {
            self.attack = Some(a);
        }
        self.sustain = Some(sustain.clamp(0.0, 1.0));
        if let Ok(r) = parse_duration(release) {
            self.release = Some(r);
        }
        self.decay = None;
        self
    }

    /// Configure an ASR envelope with f64 values.
    pub fn asr_f(mut self, attack: f64, sustain: f64, release: f64) -> Self {
        self.attack = Some(attack);
        self.sustain = Some(sustain.clamp(0.0, 1.0));
        self.release = Some(release);
        self.decay = None;
        self
    }

    /// Configure an ADSR envelope (attack → decay → sustain → release).
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings for times.
    pub fn adsr(
        mut self,
        attack: rhai::Dynamic,
        decay: rhai::Dynamic,
        sustain: f64,
        release: rhai::Dynamic,
    ) -> Self {
        if let Ok(a) = parse_duration(attack) {
            self.attack = Some(a);
        }
        if let Ok(d) = parse_duration(decay) {
            self.decay = Some(d);
        }
        self.sustain = Some(sustain.clamp(0.0, 1.0));
        if let Ok(r) = parse_duration(release) {
            self.release = Some(r);
        }
        self
    }

    /// Configure an ADSR envelope with f64 values.
    pub fn adsr_f(mut self, attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        self.attack = Some(attack);
        self.decay = Some(decay);
        self.sustain = Some(sustain.clamp(0.0, 1.0));
        self.release = Some(release);
        self
    }

    /// Configure a triangle envelope (attack to peak, then release).
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings.
    pub fn triangle(mut self, duration: rhai::Dynamic) -> Self {
        if let Ok(d) = parse_duration(duration) {
            self.attack = Some(d / 2.0);
            self.release = Some(d / 2.0);
        }
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure a triangle envelope with f64 duration.
    pub fn triangle_f(mut self, duration: f64) -> Self {
        self.attack = Some(duration / 2.0);
        self.release = Some(duration / 2.0);
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Build the envelope generator and return a NodeRef.
    ///
    /// Automatically selects the envelope type based on which parameters are set:
    /// - attack + release only → perc envelope
    /// - attack + sustain + release → asr envelope
    /// - attack + decay + sustain + release → adsr envelope
    ///
    /// If no gate is specified, defaults to dc_ar(1.0) (always on).
    pub fn build(self) -> Result<NodeRef> {
        // Default gate to dc_ar(1.0) if not provided
        let gate = match self.gate {
            Some(g) => g,
            None => {
                // Create a DC(1.0) node as the default gate
                with_builder(|builder| {
                    builder.add_constant(1.0f32);
                    let inputs = vec![Input::Constant(1.0f32)];
                    builder.add_node("DC".to_string(), Rate::Audio, inputs, 1, 0)
                })?
            }
        };

        // Determine envelope type and create Env
        let env = self.determine_envelope();

        // Build inputs with defaults
        let level_scale = self.level_scale.unwrap_or(EnvGenParam::Constant(1.0));
        let level_bias = self.level_bias.unwrap_or(EnvGenParam::Constant(0.0));
        let time_scale = self.time_scale.unwrap_or(EnvGenParam::Constant(1.0));
        let done_action = self.done_action.unwrap_or(EnvGenParam::Constant(0.0));

        // Add constants to builder before using them (required for Input::Constant to work)
        with_builder(|builder| {
            if let EnvGenParam::Constant(v) = &level_scale {
                builder.add_constant(*v as f32);
            }
            if let EnvGenParam::Constant(v) = &level_bias {
                builder.add_constant(*v as f32);
            }
            if let EnvGenParam::Constant(v) = &time_scale {
                builder.add_constant(*v as f32);
            }
            if let EnvGenParam::Constant(v) = &done_action {
                builder.add_constant(*v as f32);
            }
        })?;

        env_gen_with_env_impl(
            env,
            gate,
            level_scale.to_input(),
            level_bias.to_input(),
            time_scale.to_input(),
            done_action.to_input(),
        )
    }

    /// Determine the envelope type based on set parameters.
    fn determine_envelope(&self) -> Env {
        let attack = self.attack.unwrap_or(0.01);
        let release = self.release.unwrap_or(0.1);

        match (self.decay, self.sustain) {
            // Full ADSR
            (Some(decay), Some(sustain)) => Env::adsr(attack, decay, sustain, release),
            // ASR (no decay, has sustain)
            (None, Some(sustain)) => Env::asr(attack, sustain, release),
            // Perc (no sustain, no decay)
            _ => Env::perc(attack, release),
        }
    }
}

impl Default for EnvelopeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new envelope builder.
pub fn envelope() -> EnvelopeBuilder {
    EnvelopeBuilder::new()
}

/// Read from hardware audio input (microphone, line-in, etc.).
/// Channel 0 is the first hardware input.
/// Returns an array of audio signals, one per channel.
pub fn sound_in(num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    // SoundIn with bus=0 reads from hardware inputs starting at channel 0
    // For mono, just read channel 0; for stereo, read channels 0 and 1
    if num_ch == 1 {
        // Single channel - SoundIn.ar(0)
        let node_ref = with_builder(|builder| {
            builder.add_constant(0.0);
            let inputs = vec![Input::Constant(0.0)];
            builder.add_node("SoundIn".to_string(), Rate::Audio, inputs, 1, 0)
        })?;
        let mut result = Array::new();
        result.push(Dynamic::from(node_ref));
        Ok(result)
    } else {
        // Multiple channels - use In.ar reading from NumOutputBusChannels
        // SoundIn internally does: In.ar(NumOutputBusChannels.ir + bus, numChannels)
        // Since we have 2 output channels, hardware inputs start at bus 2
        let node_ref = with_builder(|builder| {
            builder.add_constant(2.0); // NumOutputBusChannels = 2
            builder.add_constant(num_channels as f32);
            let inputs = vec![
                Input::Constant(2.0),
                Input::Constant(num_channels as f32),
            ];
            builder.add_node("In".to_string(), Rate::Audio, inputs, num_ch, 0)
        })?;

        let mut result = Array::new();
        for ch in 0..num_ch {
            let channel_ref = NodeRef::new_with_output(node_ref.id(), ch);
            result.push(Dynamic::from(channel_ref));
        }
        Ok(result)
    }
}

/// Read from hardware audio input, single channel version.
/// Channel specifies which hardware input to read (0 = first input).
pub fn sound_in_channel(channel: f64) -> Result<NodeRef> {
    with_builder(|builder| {
        builder.add_constant(channel as f32);
        let inputs = vec![Input::Constant(channel as f32)];
        builder.add_node("SoundIn".to_string(), Rate::Audio, inputs, 1, 0)
    })
}

/// Read from an audio bus.
pub fn in_ar(bus: f64, num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    let node_ref = with_builder(|builder| {
        builder.add_constant(bus as f32);
        builder.add_constant(num_channels as f32);
        let inputs = vec![
            Input::Constant(bus as f32),
            Input::Constant(num_channels as f32),
        ];
        builder.add_node("In".to_string(), Rate::Audio, inputs, num_ch, 0)
    })?;

    let mut result = Array::new();
    for ch in 0..num_ch {
        let channel_ref = NodeRef::new_with_output(node_ref.id(), ch);
        result.push(Dynamic::from(channel_ref));
    }
    Ok(result)
}

/// Read from an audio bus (NodeRef version).
pub fn in_ar_n(bus: NodeRef, num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    let node_ref = with_builder(|builder| {
        builder.add_constant(num_channels as f32);
        let inputs = vec![bus.to_input(), Input::Constant(num_channels as f32)];
        builder.add_node("In".to_string(), Rate::Audio, inputs, num_ch, 0)
    })?;

    let mut result = Array::new();
    for ch in 0..num_ch {
        let channel_ref = NodeRef::new_with_output(node_ref.id(), ch);
        result.push(Dynamic::from(channel_ref));
    }
    Ok(result)
}

/// Write to an audio bus, replacing contents.
pub fn replace_out_ar(bus: f64, channels: Array) -> Result<NodeRef> {
    let mut inputs = vec![Input::Constant(bus as f32)];

    for ch in channels.iter() {
        if let Some(node_ref) = ch.clone().try_cast::<NodeRef>() {
            inputs.push(node_ref.to_input());
        } else {
            return Err(SynthDefError::InvalidBodyReturn);
        }
    }

    with_builder(|builder| {
        builder.add_constant(bus as f32);
        builder.add_node("ReplaceOut".to_string(), Rate::Audio, inputs, 0, 0)
    })
}

/// Write to an audio bus (NodeRef version).
pub fn replace_out_ar_n(bus: NodeRef, channels: Array) -> Result<NodeRef> {
    let mut inputs = vec![bus.to_input()];

    for ch in channels.iter() {
        if let Some(node_ref) = ch.clone().try_cast::<NodeRef>() {
            inputs.push(node_ref.to_input());
        } else {
            return Err(SynthDefError::InvalidBodyReturn);
        }
    }

    with_builder(|builder| builder.add_node("ReplaceOut".to_string(), Rate::Audio, inputs, 0, 0))
}

/// Register all helper functions with the Rhai engine.
pub fn register_helpers(engine: &mut rhai::Engine) {
    // Register Env type
    engine
        .register_type::<Env>()
        .register_fn("Env", |levels: Array, times: Array, curve: f64| {
            Env::new(levels, times, curve).unwrap()
        });

    // Register Env static methods
    engine.register_fn("env_perc", || Env::perc(0.01, 1.0));
    engine.register_fn("env_perc", |attack: f64| Env::perc(attack, 1.0));
    engine.register_fn("env_perc", |attack: f64, release: f64| Env::perc(attack, release));
    engine.register_fn("env_adsr", |attack: f64, decay: f64, sustain: f64, release: f64| {
        Env::adsr(attack, decay, sustain, release)
    });
    engine.register_fn("env_asr", |attack: f64, sustain: f64, release: f64| {
        Env::asr(attack, sustain, release)
    });
    engine.register_fn("env_triangle", |duration: f64| Env::triangle(duration));

    // Bus I/O
    engine.register_fn("in_ar", |bus: f64, num_channels: f64| in_ar(bus, num_channels).unwrap());
    engine.register_fn("in_ar", |bus: f64, num_channels: i64| in_ar(bus, num_channels as f64).unwrap());
    engine.register_fn("in_ar", |bus: i64, num_channels: f64| in_ar(bus as f64, num_channels).unwrap());
    engine.register_fn("in_ar", |bus: i64, num_channels: i64| in_ar(bus as f64, num_channels as f64).unwrap());
    engine.register_fn("in_ar", |bus: NodeRef, num_channels: f64| in_ar_n(bus, num_channels).unwrap());
    engine.register_fn("in_ar", |bus: NodeRef, num_channels: i64| in_ar_n(bus, num_channels as f64).unwrap());
    engine.register_fn("replace_out_ar", |bus: f64, channels: Array| replace_out_ar(bus, channels).unwrap());
    engine.register_fn("replace_out_ar", |bus: NodeRef, channels: Array| replace_out_ar_n(bus, channels).unwrap());

    // Hardware audio input (line-in, microphone)
    engine.register_fn("sound_in", |num_channels: f64| sound_in(num_channels).unwrap());
    engine.register_fn("sound_in", |num_channels: i64| sound_in(num_channels as f64).unwrap());
    engine.register_fn("sound_in_channel", |channel: f64| sound_in_channel(channel).unwrap());
    engine.register_fn("sound_in_channel", |channel: i64| sound_in_channel(channel as f64).unwrap());

    // Mix and utilities
    engine.register_fn("mix", |arr: Array| mix(arr).unwrap());
    engine.register_fn("sum", |arr: Array| mix(arr).unwrap()); // Alias for mix
    engine.register_fn("dup", |sig: NodeRef, count: i64| dup(sig, count).unwrap());
    engine.register_fn("channels", |sig: NodeRef, count: i64| channels(sig, count).unwrap());
    engine.register_fn("channel", |sig: NodeRef, index: i64| channel(sig, index).unwrap());
    engine.register_fn("detune_spread", |voices: i64, amount: f64| detune_spread(voices, amount).unwrap());

    // Envelopes
    engine.register_fn("env_gen", |gate: NodeRef, done: i64| env_gen(gate, done).unwrap());

    // EnvGen builder
    engine
        .register_type::<EnvGenBuilder>()
        .register_fn("NewEnvGenBuilder", EnvGenBuilder::new)
        .register_fn("with_level_scale", EnvGenBuilder::with_level_scale)
        .register_fn("with_level_scale", EnvGenBuilder::with_level_scale_n)
        .register_fn("with_level_bias", EnvGenBuilder::with_level_bias)
        .register_fn("with_level_bias", EnvGenBuilder::with_level_bias_n)
        .register_fn("with_time_scale", EnvGenBuilder::with_time_scale)
        .register_fn("with_time_scale", EnvGenBuilder::with_time_scale_n)
        .register_fn("with_done_action", EnvGenBuilder::with_done_action)
        .register_fn("with_done_action", EnvGenBuilder::with_done_action_n)
        .register_fn("build", |builder: EnvGenBuilder| EnvGenBuilder::build(builder).unwrap());

    // New fluent EnvelopeBuilder API
    engine
        .register_type::<EnvelopeBuilder>()
        .register_fn("envelope", envelope)
        // Attack methods
        .register_fn("attack", EnvelopeBuilder::attack)
        .register_fn("attack", EnvelopeBuilder::attack_f)
        // Decay methods
        .register_fn("decay", EnvelopeBuilder::decay)
        .register_fn("decay", EnvelopeBuilder::decay_f)
        // Sustain method
        .register_fn("sustain", EnvelopeBuilder::sustain)
        // Release methods
        .register_fn("release", EnvelopeBuilder::release)
        .register_fn("release", EnvelopeBuilder::release_f)
        // Gate method
        .register_fn("gate", EnvelopeBuilder::gate)
        // Done action methods
        .register_fn("cleanup_on_finish", EnvelopeBuilder::cleanup_on_finish)
        .register_fn("done_action", EnvelopeBuilder::done_action)
        .register_fn("done_action", EnvelopeBuilder::done_action_n)
        // Scale/bias methods
        .register_fn("level_scale", EnvelopeBuilder::level_scale)
        .register_fn("level_scale", EnvelopeBuilder::level_scale_n)
        .register_fn("level_bias", EnvelopeBuilder::level_bias)
        .register_fn("level_bias", EnvelopeBuilder::level_bias_n)
        .register_fn("time_scale", EnvelopeBuilder::time_scale)
        .register_fn("time_scale", EnvelopeBuilder::time_scale_n)
        // Convenience envelope methods
        .register_fn("perc", EnvelopeBuilder::perc)
        .register_fn("perc", EnvelopeBuilder::perc_f)
        .register_fn("asr", EnvelopeBuilder::asr)
        .register_fn("asr", EnvelopeBuilder::asr_f)
        .register_fn("adsr", EnvelopeBuilder::adsr)
        .register_fn("adsr", EnvelopeBuilder::adsr_f)
        .register_fn("triangle", EnvelopeBuilder::triangle)
        .register_fn("triangle", EnvelopeBuilder::triangle_f)
        // Build method
        .register_fn("build", |builder: EnvelopeBuilder| EnvelopeBuilder::build(builder).unwrap());

    // EnvGen with Env
    engine.register_fn(
        "env_gen",
        |env: Env, gate: NodeRef, level_scale: f64, level_bias: f64, time_scale: f64, done_action: f64| {
            env_gen_with_env(env, gate, level_scale, level_bias, time_scale, done_action).unwrap()
        },
    );
    engine.register_fn(
        "env_gen",
        |env: Env, gate: NodeRef, level_scale: NodeRef, level_bias: NodeRef, time_scale: NodeRef, done_action: NodeRef| {
            env_gen_with_env_n(env, gate, level_scale, level_bias, time_scale, done_action).unwrap()
        },
    );

    // Math
    engine.register_fn("db_to_amp", db_to_amp);
    engine.register_fn("amp_to_db", amp_to_db);
    engine.register_fn("pow", |base: f64, exp: f64| base.powf(exp));
    engine.register_fn("log", |val: f64| val.ln());
    engine.register_fn("log10", |val: f64| val.log10());
    engine.register_fn("log2", |val: f64| val.log2());
    engine.register_fn("sqrt", |val: f64| val.sqrt());
    engine.register_fn("abs", |val: f64| val.abs());
    engine.register_fn("floor", |val: f64| val.floor());
    engine.register_fn("ceil", |val: f64| val.ceil());
    engine.register_fn("round", |val: f64| val.round());
    engine.register_fn("min", |a: f64, b: f64| a.min(b));
    engine.register_fn("max", |a: f64, b: f64| a.max(b));
    engine.register_fn("clamp", |val: f64, lo: f64, hi: f64| val.clamp(lo, hi));

    // DC offset generator
    engine.register_fn("dc_ar", |val: f64| {
        with_builder(|builder| {
            builder.add_constant(val as f32);
            let inputs = vec![Input::Constant(val as f32)];
            builder.add_node("DC".to_string(), Rate::Audio, inputs, 1, 0)
        })
        .unwrap()
    });
    engine.register_fn("dc_kr", |val: f64| {
        with_builder(|builder| {
            builder.add_constant(val as f32);
            let inputs = vec![Input::Constant(val as f32)];
            builder.add_node("DC".to_string(), Rate::Control, inputs, 1, 0)
        })
        .unwrap()
    });
}
