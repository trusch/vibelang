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

pub(crate) fn dynamic_to_signal_inputs(value: &Dynamic) -> Result<Vec<Input>> {
    if let Some(array) = value.clone().try_cast::<Array>() {
        if array.is_empty() {
            return Err(SynthDefError::InvalidBodyReturn);
        }
        array.iter().map(dynamic_to_input).collect()
    } else {
        Ok(vec![dynamic_to_input(value)?])
    }
}

fn dynamic_to_f64(value: &Dynamic) -> Result<f64> {
    if let Some(f) = value.clone().try_cast::<f64>() {
        Ok(f)
    } else if let Some(i) = value.clone().try_cast::<i64>() {
        Ok(i as f64)
    } else if let Some(i) = value.clone().try_cast::<i32>() {
        Ok(i as f64)
    } else {
        Err(SynthDefError::ValidationError(format!(
            "Expected numeric value, got {}",
            value.type_name()
        )))
    }
}

pub fn dynamic_to_shape_count(value: &Dynamic) -> Result<u32> {
    let count = dynamic_to_f64(value)?;
    if !count.is_finite() || count.fract() != 0.0 || count < 1.0 || count > i16::MAX as f64 {
        return Err(SynthDefError::ValidationError(format!(
            "Expected channel count integer in 1..={}, got {}",
            i16::MAX,
            count
        )));
    }
    Ok(count as u32)
}

fn add_constant_input(builder: &mut GraphBuilderInner, value: f32) -> Input {
    builder.add_constant(value);
    Input::Constant(value)
}

fn add_unary_op(builder: &mut GraphBuilderInner, input: Input, special_index: i16) -> NodeRef {
    let rate = builder.max_rate_from_inputs(std::slice::from_ref(&input));
    builder.add_node(
        "UnaryOpUGen".to_string(),
        rate,
        vec![input],
        1,
        special_index,
    )
}

fn add_binary_op(
    builder: &mut GraphBuilderInner,
    left: Input,
    right: Input,
    special_index: i16,
) -> NodeRef {
    let inputs = vec![left, right];
    let rate = builder.max_rate_from_inputs(&inputs);
    builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, special_index)
}

fn input_to_node(builder: &mut GraphBuilderInner, input: Input, target_rate: Rate) -> NodeRef {
    match input {
        Input::Constant(value) => {
            let input = add_constant_input(builder, value);
            builder.add_node("DC".to_string(), target_rate, vec![input], 1, 0)
        }
        Input::Node {
            node_id,
            output_index,
        } => NodeRef::new_with_output(node_id, output_index),
    }
}

fn coerce_node_to_rate(
    builder: &mut GraphBuilderInner,
    node: NodeRef,
    target_rate: Rate,
) -> NodeRef {
    let source_rate = builder.get_node_rate(node.id());
    match (source_rate, target_rate) {
        (Rate::Control, Rate::Audio) | (Rate::Scalar, Rate::Audio) => {
            builder.add_node("K2A".to_string(), Rate::Audio, vec![node.to_input()], 1, 0)
        }
        (Rate::Audio, Rate::Control) => builder.add_node(
            "A2K".to_string(),
            Rate::Control,
            vec![node.to_input()],
            1,
            0,
        ),
        _ => node,
    }
}

fn mix_inputs_as_node(
    builder: &mut GraphBuilderInner,
    inputs: Vec<Input>,
    target_rate: Rate,
) -> NodeRef {
    let mut iter = inputs.into_iter();
    let first = iter
        .next()
        .expect("mix_inputs_as_node requires at least one input");
    let mut result = input_to_node(builder, first, target_rate);

    for input in iter {
        result = add_binary_op(builder, result.to_input(), input, 0);
    }

    coerce_node_to_rate(builder, result, target_rate)
}

fn first_last_inputs(value: &Dynamic) -> Result<(Input, Input)> {
    let inputs = dynamic_to_signal_inputs(value)?;
    let first = inputs.first().expect("non-empty signal input").clone();
    let last = inputs.last().expect("non-empty signal input").clone();
    Ok((first, last))
}

fn level_compensation(rate: Rate, channel_count: usize, level_comp: &Dynamic) -> f32 {
    if let Some(enabled) = level_comp.clone().try_cast::<bool>() {
        if !enabled {
            return 1.0;
        }
        return match rate {
            Rate::Audio => (channel_count as f32).powf(-0.5),
            Rate::Control | Rate::Scalar => (channel_count as f32).powf(-1.0),
        };
    }

    let exponent = dynamic_to_f64(level_comp)
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(1.0);
    if exponent == 0.0 {
        1.0
    } else {
        (channel_count as f32).powf(-(exponent as f32))
    }
}

fn scaled_level_input(builder: &mut GraphBuilderInner, level: Input, compensation: f32) -> Input {
    if (compensation - 1.0).abs() < f32::EPSILON {
        level
    } else {
        let factor = add_constant_input(builder, compensation);
        add_binary_op(builder, level, factor, 2).to_input()
    }
}

/// Lower sclang's `Changed` pseudo-UGen (`Filter.sc`) to HPZ1 + abs + >.
pub fn changed_pseudo(rate: Rate, input: &Dynamic, threshold: &Dynamic) -> Result<NodeRef> {
    let input = dynamic_to_input(input)?;
    let threshold = dynamic_to_input(threshold)?;

    with_builder(|builder| {
        let hpz1 = builder.add_node("HPZ1".to_string(), rate, vec![input], 1, 0);
        let abs = add_unary_op(builder, hpz1.to_input(), 5);
        add_binary_op(builder, abs.to_input(), threshold, 9)
    })
}

/// Lower sclang's `LinLin` helper to the supported MulAdd arithmetic graph.
pub fn lin_lin_pseudo(
    rate: Rate,
    input: &Dynamic,
    srclo: &Dynamic,
    srchi: &Dynamic,
    dstlo: &Dynamic,
    dsthi: &Dynamic,
) -> Result<NodeRef> {
    let input = dynamic_to_input(input)?;
    let srclo = dynamic_to_input(srclo)?;
    let srchi = dynamic_to_input(srchi)?;
    let dstlo = dynamic_to_input(dstlo)?;
    let dsthi = dynamic_to_input(dsthi)?;

    with_builder(|builder| {
        let dst_span = add_binary_op(builder, dsthi, dstlo.clone(), 1);
        let src_span = add_binary_op(builder, srchi, srclo.clone(), 1);
        let scale = add_binary_op(builder, dst_span.to_input(), src_span.to_input(), 4);
        let scaled_srclo = add_binary_op(builder, scale.to_input(), srclo, 2);
        let offset = add_binary_op(builder, dstlo, scaled_srclo.to_input(), 1);
        builder.add_node(
            "MulAdd".to_string(),
            rate,
            vec![input, scale.to_input(), offset.to_input()],
            1,
            0,
        )
    })
}

/// Lower sc3-plugins' `JPverb` wrapper to the installed raw binary UGen.
pub fn jpverb_pseudo(
    input: &Dynamic,
    t60: &Dynamic,
    damp: &Dynamic,
    size: &Dynamic,
    early_diff: &Dynamic,
    mod_depth: &Dynamic,
    mod_freq: &Dynamic,
    low: &Dynamic,
    mid: &Dynamic,
    high: &Dynamic,
    lowcut: &Dynamic,
    highcut: &Dynamic,
) -> Result<NodeRef> {
    let (first, last) = first_last_inputs(input)?;
    let inputs = vec![
        first,
        last,
        dynamic_to_input(damp)?,
        dynamic_to_input(early_diff)?,
        dynamic_to_input(highcut)?,
        dynamic_to_input(high)?,
        dynamic_to_input(lowcut)?,
        dynamic_to_input(low)?,
        dynamic_to_input(mod_depth)?,
        dynamic_to_input(mod_freq)?,
        dynamic_to_input(mid)?,
        dynamic_to_input(size)?,
        dynamic_to_input(t60)?,
    ];

    with_builder(|builder| builder.add_node("JPverbRaw".to_string(), Rate::Audio, inputs, 2, 0))
}

/// Lower sc3-plugins' `Greyhole` wrapper to the installed raw binary UGen.
pub fn greyhole_pseudo(
    input: &Dynamic,
    delay_time: &Dynamic,
    damp: &Dynamic,
    size: &Dynamic,
    diff: &Dynamic,
    feedback: &Dynamic,
    mod_depth: &Dynamic,
    mod_freq: &Dynamic,
) -> Result<NodeRef> {
    let (first, last) = first_last_inputs(input)?;
    let inputs = vec![
        first,
        last,
        dynamic_to_input(damp)?,
        dynamic_to_input(delay_time)?,
        dynamic_to_input(diff)?,
        dynamic_to_input(feedback)?,
        dynamic_to_input(mod_depth)?,
        dynamic_to_input(mod_freq)?,
        dynamic_to_input(size)?,
    ];

    with_builder(|builder| builder.add_node("GreyholeRaw".to_string(), Rate::Audio, inputs, 2, 0))
}

/// Lower `Mix.ar` / `Mix.kr` to a normal add tree and rate coercion.
pub fn mix_pseudo(rate: Rate, array: &Dynamic) -> Result<Dynamic> {
    let inputs = dynamic_to_signal_inputs(array)?;
    let node = with_builder(|builder| mix_inputs_as_node(builder, inputs, rate))?;
    Ok(Dynamic::from(node))
}

/// Lower `Splay` to Pan2 over each input, followed by per-channel Mix.
pub fn splay_pseudo(
    rate: Rate,
    in_array: &Dynamic,
    spread: &Dynamic,
    level: &Dynamic,
    center: &Dynamic,
    level_comp: &Dynamic,
) -> Result<Dynamic> {
    let inputs = dynamic_to_signal_inputs(in_array)?;
    let spread = dynamic_to_input(spread)?;
    let level = dynamic_to_input(level)?;
    let center = dynamic_to_input(center)?;
    let pan_count = inputs.len();
    let n = pan_count.max(2);
    let compensation = level_compensation(rate, n, level_comp);

    let channels = with_builder(|builder| {
        let level = scaled_level_input(builder, level, compensation);
        let mut left = Vec::with_capacity(pan_count);
        let mut right = Vec::with_capacity(pan_count);
        let denominator = (n - 1) as f32;

        for (index, input) in inputs.into_iter().enumerate() {
            let base_position = (index as f32 * (2.0 / denominator)) - 1.0;
            let base_position = add_constant_input(builder, base_position);
            let scaled_spread = add_binary_op(builder, base_position, spread.clone(), 2);
            let position = add_binary_op(builder, scaled_spread.to_input(), center.clone(), 0);
            let pan_level = add_constant_input(builder, 1.0);
            let pan = builder.add_node(
                "Pan2".to_string(),
                rate,
                vec![input, position.to_input(), pan_level],
                2,
                0,
            );
            left.push(NodeRef::new_with_output(pan.id(), 0).to_input());
            right.push(NodeRef::new_with_output(pan.id(), 1).to_input());
        }

        let left = mix_inputs_as_node(builder, left, rate);
        let right = mix_inputs_as_node(builder, right, rate);
        let left = add_binary_op(builder, left.to_input(), level.clone(), 2);
        let right = add_binary_op(builder, right.to_input(), level, 2);
        let mut result = Array::new();
        result.push(Dynamic::from(left));
        result.push(Dynamic::from(right));
        result
    })?;

    Ok(Dynamic::from(channels))
}

/// Lower `SplayAz` to PanAz over each input, followed by per-output Mix.
pub fn splay_az_pseudo(
    rate: Rate,
    num_chans: &Dynamic,
    in_array: &Dynamic,
    spread: &Dynamic,
    level: &Dynamic,
    width: &Dynamic,
    center: &Dynamic,
    orientation: &Dynamic,
    level_comp: &Dynamic,
) -> Result<Dynamic> {
    let output_count = dynamic_to_f64(num_chans)? as u32;
    if output_count == 0 {
        return Err(SynthDefError::ValidationError(
            "splay_az numChans must be positive".to_string(),
        ));
    }

    let inputs = dynamic_to_signal_inputs(in_array)?;
    let spread = dynamic_to_input(spread)?;
    let level = dynamic_to_input(level)?;
    let width = dynamic_to_input(width)?;
    let center = dynamic_to_input(center)?;
    let orientation = dynamic_to_input(orientation)?;
    let pan_count = inputs.len().max(1);
    let compensation = level_compensation(rate, pan_count, level_comp);

    let channels = with_builder(|builder| {
        let level = scaled_level_input(builder, level, compensation);
        let num_chans_input = add_constant_input(builder, output_count as f32);
        let mut columns: Vec<Vec<Input>> = (0..output_count).map(|_| Vec::new()).collect();
        let norm_spread = if pan_count == 1 {
            0.0
        } else {
            ((pan_count - 1) as f32) / (pan_count as f32)
        };

        for (index, input) in inputs.into_iter().enumerate() {
            let position = if pan_count == 1 {
                input_to_node(builder, center.clone(), rate)
            } else {
                let t = index as f32 / ((pan_count - 1) as f32);
                let coefficient = -norm_spread + (2.0 * norm_spread * t);
                let coefficient = add_constant_input(builder, coefficient);
                let scaled_spread = add_binary_op(builder, coefficient, spread.clone(), 2);
                add_binary_op(builder, center.clone(), scaled_spread.to_input(), 0)
            };
            let pan = builder.add_node(
                "PanAz".to_string(),
                rate,
                vec![
                    num_chans_input.clone(),
                    input,
                    position.to_input(),
                    level.clone(),
                    width.clone(),
                    orientation.clone(),
                ],
                output_count,
                0,
            );
            for output in 0..output_count {
                columns[output as usize]
                    .push(NodeRef::new_with_output(pan.id(), output).to_input());
            }
        }

        let mut result = Array::new();
        for column in columns {
            result.push(Dynamic::from(mix_inputs_as_node(builder, column, rate)));
        }
        result
    })?;

    Ok(Dynamic::from(channels))
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

/// Zip two arrays together into an array of pairs.
pub fn array_zip(arr1: Array, arr2: Array) -> Array {
    arr1.into_iter()
        .zip(arr2)
        .map(|(a, b)| Dynamic::from(vec![a, b]))
        .collect()
}

/// Envelope specification (like SuperCollider's Env class).
#[derive(Clone, Debug)]
pub struct Env {
    pub levels: Vec<EnvGenParam>,
    pub times: Vec<EnvGenParam>,
    pub curves: Vec<f32>,
    pub release_node: i32,
}

impl Env {
    /// Create a new envelope specification.
    ///
    /// Both levels and times accept either numbers or control sources
    /// (`NodeRef`), so a synthdef parameter can drive a breakpoint's level
    /// just as it can drive its duration.
    pub fn new(levels: Array, times: Array, curve: f64) -> Result<Self> {
        let levels_vec: Vec<EnvGenParam> = levels
            .iter()
            .map(|v| env_gen_param_from_dynamic(v.clone(), "level"))
            .collect::<Result<Vec<_>>>()?;
        let times_vec: Vec<EnvGenParam> = times
            .iter()
            .map(|v| env_gen_param_from_dynamic(v.clone(), "time"))
            .collect::<Result<Vec<_>>>()?;

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
        Self::perc_p(
            EnvGenParam::Constant(attack),
            EnvGenParam::Constant(release),
        )
    }

    pub fn perc_p(attack: EnvGenParam, release: EnvGenParam) -> Self {
        Env {
            levels: vec![
                EnvGenParam::Constant(0.0),
                EnvGenParam::Constant(1.0),
                EnvGenParam::Constant(0.0),
            ],
            times: vec![attack, release],
            curves: vec![1.0, 1.0],
            release_node: -1,
        }
    }

    /// Create an ADSR envelope.
    pub fn adsr(attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        Self::adsr_p(
            EnvGenParam::Constant(attack),
            EnvGenParam::Constant(decay),
            EnvGenParam::Constant(sustain),
            EnvGenParam::Constant(release),
        )
    }

    /// ADSR where any of the four arguments may be a control source.
    ///
    /// The sustain level is a level rather than a time, so it lands in
    /// `levels`; it is the reason `levels` carries `EnvGenParam`.
    pub fn adsr_p(
        attack: EnvGenParam,
        decay: EnvGenParam,
        sustain: EnvGenParam,
        release: EnvGenParam,
    ) -> Self {
        Env {
            levels: vec![
                EnvGenParam::Constant(0.0),
                EnvGenParam::Constant(1.0),
                sustain.clone(),
                sustain,
                EnvGenParam::Constant(0.0),
            ],
            times: vec![attack, decay, EnvGenParam::Constant(0.0), release],
            curves: vec![1.0, 1.0, 1.0, 1.0],
            release_node: 3,
        }
    }

    /// Create an ASR envelope.
    pub fn asr(attack: f64, sustain: f64, release: f64) -> Self {
        Self::asr_p(
            EnvGenParam::Constant(attack),
            EnvGenParam::Constant(sustain),
            EnvGenParam::Constant(release),
        )
    }

    /// ASR where any of the three arguments may be a control source.
    pub fn asr_p(attack: EnvGenParam, sustain: EnvGenParam, release: EnvGenParam) -> Self {
        Env {
            levels: vec![
                EnvGenParam::Constant(0.0),
                sustain,
                EnvGenParam::Constant(0.0),
            ],
            times: vec![attack, release],
            curves: vec![1.0, 1.0],
            release_node: 1,
        }
    }

    /// Create a triangle envelope.
    pub fn triangle(duration: f64) -> Self {
        Env {
            levels: vec![
                EnvGenParam::Constant(0.0),
                EnvGenParam::Constant(1.0),
                EnvGenParam::Constant(0.0),
            ],
            times: vec![
                EnvGenParam::Constant(duration / 2.0),
                EnvGenParam::Constant(duration / 2.0),
            ],
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
        let init_level = &env.levels[0];
        let release_node = env.release_node as f32;
        let loop_node = -1.0f32;

        if let EnvGenParam::Constant(l) = init_level {
            builder.add_constant(*l as f32);
        }
        builder.add_constant(num_stages);
        builder.add_constant(release_node);
        builder.add_constant(loop_node);

        for i in 0..(num_levels - 1) {
            if let EnvGenParam::Constant(l) = &env.levels[i + 1] {
                builder.add_constant(*l as f32);
            }
            if let EnvGenParam::Constant(t) = &env.times[i] {
                builder.add_constant(*t as f32);
            }
            let curve_val = env.curves[i];
            let shape = if curve_val == 1.0 {
                1.0
            } else if curve_val == 2.0 {
                2.0
            } else {
                5.0
            };
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

        inputs.push(init_level.to_input());
        inputs.push(Input::Constant(num_stages));
        inputs.push(Input::Constant(release_node));
        inputs.push(Input::Constant(loop_node));

        for i in 0..(num_levels - 1) {
            inputs.push(env.levels[i + 1].to_input());
            inputs.push(env.times[i].to_input());
            let curve_val = env.curves[i];
            let shape = if curve_val == 1.0 {
                1.0
            } else if curve_val == 2.0 {
                2.0
            } else {
                5.0
            };
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

/// Coerce one entry of an `Env(levels, times, curve)` array into an
/// `EnvGenParam`. `kind` is "level" or "time" and only shapes the error.
fn env_gen_param_from_dynamic(value: rhai::Dynamic, kind: &str) -> Result<EnvGenParam> {
    if let Some(node) = value.clone().try_cast::<NodeRef>() {
        Ok(EnvGenParam::Node(node))
    } else if let Some(f) = value.clone().try_cast::<f64>() {
        Ok(EnvGenParam::Constant(f))
    } else if let Some(i) = value.try_cast::<i64>() {
        Ok(EnvGenParam::Constant(i as f64))
    } else {
        Err(SynthDefError::ValidationError(format!(
            "Env {} must be a number or control source",
            kind
        )))
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

fn synthdef_error_to_eval(err: SynthDefError) -> Box<rhai::EvalAltResult> {
    Box::new(rhai::EvalAltResult::ErrorRuntime(
        err.to_string().into(),
        rhai::Position::NONE,
    ))
}

fn set_env_error(error: &mut Option<String>, field: &str, message: String) {
    let message = format!("envelope {}: {}", field, message);
    log::warn!("{}", message);
    *error = Some(message);
}

fn parse_time_param(
    error: &mut Option<String>,
    value: rhai::Dynamic,
    field: &str,
) -> Option<EnvGenParam> {
    if let Some(node) = value.clone().try_cast::<NodeRef>() {
        Some(EnvGenParam::Node(node))
    } else {
        match parse_duration(value) {
            Ok(t) => Some(EnvGenParam::Constant(t)),
            Err(e) => {
                set_env_error(error, field, e.to_string());
                None
            }
        }
    }
}

/// Parse an envelope *level* (as opposed to a time).
///
/// Accepts a control source or a number. Numeric levels are clamped to
/// 0.0..=1.0, matching the long-standing `sustain(f64)` behaviour;
/// control-rate levels cannot be clamped at graph-build time and are
/// passed through as the caller supplied them.
fn parse_level_param(
    error: &mut Option<String>,
    value: rhai::Dynamic,
    field: &str,
) -> Option<EnvGenParam> {
    if let Some(node) = value.clone().try_cast::<NodeRef>() {
        Some(EnvGenParam::Node(node))
    } else if let Some(v) = value.clone().try_cast::<f64>() {
        Some(EnvGenParam::Constant(v.clamp(0.0, 1.0)))
    } else if let Some(v) = value.try_cast::<i64>() {
        Some(EnvGenParam::Constant((v as f64).clamp(0.0, 1.0)))
    } else {
        set_env_error(
            error,
            field,
            "Expected number or control source".to_string(),
        );
        None
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
    attack: Option<EnvGenParam>,
    decay: Option<EnvGenParam>,
    sustain: Option<EnvGenParam>,
    release: Option<EnvGenParam>,
    error: Option<String>,

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
            error: None,
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
        if let Some(t) = parse_time_param(&mut self.error, value, "attack") {
            self.attack = Some(t);
        }
        self
    }

    /// Set the attack time from f64 (seconds).
    pub fn attack_f(mut self, seconds: f64) -> Self {
        self.attack = Some(EnvGenParam::Constant(seconds));
        self
    }

    /// Set the decay time.
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings like "5ms", "1s".
    pub fn decay(mut self, value: rhai::Dynamic) -> Self {
        if let Some(t) = parse_time_param(&mut self.error, value, "decay") {
            self.decay = Some(t);
        }
        self
    }

    /// Set the decay time from f64 (seconds).
    pub fn decay_f(mut self, seconds: f64) -> Self {
        self.decay = Some(EnvGenParam::Constant(seconds));
        self
    }

    /// Set the sustain level.
    /// Accepts f64/i64 (clamped to 0.0..=1.0) or a control source, so a
    /// synthdef parameter can drive the sustain level.
    pub fn sustain(mut self, value: rhai::Dynamic) -> Self {
        if let Some(s) = parse_level_param(&mut self.error, value, "sustain") {
            self.sustain = Some(s);
        }
        self
    }

    /// Set the sustain level from f64 (0.0 to 1.0).
    pub fn sustain_f(mut self, level: f64) -> Self {
        self.sustain = Some(EnvGenParam::Constant(level.clamp(0.0, 1.0)));
        self
    }

    /// Set the release time.
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings like "5ms", "1s".
    pub fn release(mut self, value: rhai::Dynamic) -> Self {
        if let Some(t) = parse_time_param(&mut self.error, value, "release") {
            self.release = Some(t);
        }
        self
    }

    /// Set the release time from f64 (seconds).
    pub fn release_f(mut self, seconds: f64) -> Self {
        self.release = Some(EnvGenParam::Constant(seconds));
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
        if let Some(a) = parse_time_param(&mut self.error, attack, "attack") {
            self.attack = Some(a);
        }
        if let Some(r) = parse_time_param(&mut self.error, release, "release") {
            self.release = Some(r);
        }
        // Clear decay and sustain to ensure perc envelope
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure a percussive envelope with f64 values.
    pub fn perc_f(mut self, attack: f64, release: f64) -> Self {
        self.attack = Some(EnvGenParam::Constant(attack));
        self.release = Some(EnvGenParam::Constant(release));
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure an ASR envelope (attack → sustain → release).
    /// Times accept f64 (seconds), i64 (seconds), or humantime strings;
    /// the sustain level accepts a number or a control source.
    pub fn asr(
        mut self,
        attack: rhai::Dynamic,
        sustain: rhai::Dynamic,
        release: rhai::Dynamic,
    ) -> Self {
        if let Some(a) = parse_time_param(&mut self.error, attack, "attack") {
            self.attack = Some(a);
        }
        if let Some(s) = parse_level_param(&mut self.error, sustain, "sustain") {
            self.sustain = Some(s);
        }
        if let Some(r) = parse_time_param(&mut self.error, release, "release") {
            self.release = Some(r);
        }
        self.decay = None;
        self
    }

    /// Configure an ASR envelope with f64 values.
    pub fn asr_f(mut self, attack: f64, sustain: f64, release: f64) -> Self {
        self.attack = Some(EnvGenParam::Constant(attack));
        self.sustain = Some(EnvGenParam::Constant(sustain.clamp(0.0, 1.0)));
        self.release = Some(EnvGenParam::Constant(release));
        self.decay = None;
        self
    }

    /// Configure an ADSR envelope (attack → decay → sustain → release).
    /// Times accept f64 (seconds), i64 (seconds), or humantime strings;
    /// the sustain level accepts a number or a control source, so
    /// `adsr(p.attack, p.decay, p.sustain, p.release)` is a valid synthdef.
    pub fn adsr(
        mut self,
        attack: rhai::Dynamic,
        decay: rhai::Dynamic,
        sustain: rhai::Dynamic,
        release: rhai::Dynamic,
    ) -> Self {
        if let Some(a) = parse_time_param(&mut self.error, attack, "attack") {
            self.attack = Some(a);
        }
        if let Some(d) = parse_time_param(&mut self.error, decay, "decay") {
            self.decay = Some(d);
        }
        if let Some(s) = parse_level_param(&mut self.error, sustain, "sustain") {
            self.sustain = Some(s);
        }
        if let Some(r) = parse_time_param(&mut self.error, release, "release") {
            self.release = Some(r);
        }
        self
    }

    /// Configure an ADSR envelope with f64 values.
    pub fn adsr_f(mut self, attack: f64, decay: f64, sustain: f64, release: f64) -> Self {
        self.attack = Some(EnvGenParam::Constant(attack));
        self.decay = Some(EnvGenParam::Constant(decay));
        self.sustain = Some(EnvGenParam::Constant(sustain.clamp(0.0, 1.0)));
        self.release = Some(EnvGenParam::Constant(release));
        self
    }

    /// Configure a triangle envelope (attack to peak, then release).
    /// Accepts f64 (seconds), i64 (seconds), or humantime strings.
    pub fn triangle(mut self, duration: rhai::Dynamic) -> Self {
        if duration.clone().try_cast::<NodeRef>().is_some() {
            set_env_error(
                &mut self.error,
                "triangle",
                "control-rate duration not supported in v1".to_string(),
            );
        } else {
            match parse_duration(duration) {
                Ok(d) => {
                    self.attack = Some(EnvGenParam::Constant(d / 2.0));
                    self.release = Some(EnvGenParam::Constant(d / 2.0));
                }
                Err(e) => set_env_error(&mut self.error, "triangle", e.to_string()),
            }
        }
        self.decay = None;
        self.sustain = None;
        self
    }

    /// Configure a triangle envelope with f64 duration.
    pub fn triangle_f(mut self, duration: f64) -> Self {
        self.attack = Some(EnvGenParam::Constant(duration / 2.0));
        self.release = Some(EnvGenParam::Constant(duration / 2.0));
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
        if let Some(e) = &self.error {
            return Err(SynthDefError::ValidationError(e.clone()));
        }

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
    pub fn determine_envelope(&self) -> Env {
        let attack = self.attack.clone().unwrap_or(EnvGenParam::Constant(0.01));
        let release = self.release.clone().unwrap_or(EnvGenParam::Constant(0.1));

        match (self.decay.clone(), self.sustain.clone()) {
            // Full ADSR
            (Some(decay), Some(sustain)) => Env::adsr_p(attack, decay, sustain, release),
            // ASR (no decay, has sustain)
            (None, Some(sustain)) => Env::asr_p(attack, sustain, release),
            // Perc (no sustain, no decay)
            _ => Env::perc_p(attack, release),
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
///
/// `channel` 0 is the first hardware input. Reads `num_channels` consecutive
/// inputs starting there. The bus offset is computed at synth-instantiation
/// time via the `NumOutputBuses` UGen (`In.ar(NumOutputBuses.ir + channel,
/// num_channels)`), so the synthdef adapts to whatever `-o` value scsynth
/// was started with — no hardcoded layout.
///
/// `SoundIn` is sclang-only and does not exist as a server-side UGen; we
/// emit the same expansion the SC class library does.
pub fn sound_in(num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    let in_node = with_builder(|builder| {
        let num_outs = builder.add_node("NumOutputBuses".to_string(), Rate::Scalar, vec![], 1, 0);
        builder.add_node(
            "In".to_string(),
            Rate::Audio,
            vec![num_outs.to_input()],
            num_ch,
            0,
        )
    })?;

    let mut result = Array::new();
    for ch in 0..num_ch {
        let channel_ref = NodeRef::new_with_output(in_node.id(), ch);
        result.push(Dynamic::from(channel_ref));
    }
    Ok(result)
}

/// Read from a single hardware audio input by channel index.
///
/// Same expansion as `sound_in` but for one channel: `In.ar(NumOutputBuses.ir
/// + channel, 1)`. SC reads the bus index once at synth creation, so changing
/// `channel` after the synth is running has no effect — but it's fine for
/// per-instance selection via `set_param` at instantiation.
pub fn sound_in_channel(channel: f64) -> Result<NodeRef> {
    with_builder(|builder| {
        let num_outs = builder.add_node("NumOutputBuses".to_string(), Rate::Scalar, vec![], 1, 0);
        builder.add_constant(channel as f32);
        let bus = builder.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Scalar,
            vec![num_outs.to_input(), Input::Constant(channel as f32)],
            1,
            0,
        );
        builder.add_node("In".to_string(), Rate::Audio, vec![bus.to_input()], 1, 0)
    })
}

/// Read from a hardware audio input where the channel index is a synthdef
/// parameter or another node (NodeRef variant of `sound_in_channel`).
///
/// Lets a synthdef expose `channel` as a parameter and have the runtime
/// handle the offset arithmetic, so script authors never need to know
/// scsynth's `-o` value.
pub fn sound_in_channel_n(channel: NodeRef) -> Result<NodeRef> {
    with_builder(|builder| {
        let num_outs = builder.add_node("NumOutputBuses".to_string(), Rate::Scalar, vec![], 1, 0);
        let inputs = vec![num_outs.to_input(), channel.to_input()];
        let rate = builder.max_rate_from_inputs(&inputs);
        let bus = builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 0);
        builder.add_node("In".to_string(), Rate::Audio, vec![bus.to_input()], 1, 0)
    })
}

/// Read from an audio bus.
///
/// Note: SuperCollider's In UGen only takes the bus as an input.
/// The number of channels is determined by the output count, not by an input parameter.
pub fn in_ar(bus: f64, num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    let node_ref = with_builder(|builder| {
        builder.add_constant(bus as f32);
        // Only pass bus as input - num_channels is determined by output count
        let inputs = vec![Input::Constant(bus as f32)];
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
///
/// Note: SuperCollider's In UGen only takes the bus as an input.
/// The number of channels is determined by the output count, not by an input parameter.
pub fn in_ar_n(bus: NodeRef, num_channels: f64) -> Result<Array> {
    let num_ch = num_channels as u32;
    let node_ref = with_builder(|builder| {
        // Only pass bus as input - num_channels is determined by output count
        let inputs = vec![bus.to_input()];
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
    engine.register_type::<Env>().register_fn(
        "Env",
        |levels: Array,
         times: Array,
         curve: f64|
         -> std::result::Result<Env, Box<rhai::EvalAltResult>> {
            Env::new(levels, times, curve).map_err(synthdef_error_to_eval)
        },
    );

    // Register Env static methods
    engine.register_fn("env_perc", || Env::perc(0.01, 1.0));
    engine.register_fn("env_perc", |attack: f64| Env::perc(attack, 1.0));
    engine.register_fn("env_perc", |attack: f64, release: f64| {
        Env::perc(attack, release)
    });
    engine.register_fn(
        "env_adsr",
        |attack: f64, decay: f64, sustain: f64, release: f64| {
            Env::adsr(attack, decay, sustain, release)
        },
    );
    // Sustain as a control source (synthdef parameter).
    engine.register_fn(
        "env_adsr",
        |attack: f64, decay: f64, sustain: NodeRef, release: f64| {
            Env::adsr_p(
                EnvGenParam::Constant(attack),
                EnvGenParam::Constant(decay),
                EnvGenParam::Node(sustain),
                EnvGenParam::Constant(release),
            )
        },
    );
    engine.register_fn("env_asr", |attack: f64, sustain: f64, release: f64| {
        Env::asr(attack, sustain, release)
    });
    engine.register_fn("env_asr", |attack: f64, sustain: NodeRef, release: f64| {
        Env::asr_p(
            EnvGenParam::Constant(attack),
            EnvGenParam::Node(sustain),
            EnvGenParam::Constant(release),
        )
    });
    engine.register_fn("env_triangle", |duration: f64| Env::triangle(duration));

    // Bus I/O
    engine.register_fn("in_ar", |bus: f64, num_channels: f64| {
        in_ar(bus, num_channels).unwrap()
    });
    engine.register_fn("in_ar", |bus: f64, num_channels: i64| {
        in_ar(bus, num_channels as f64).unwrap()
    });
    engine.register_fn("in_ar", |bus: i64, num_channels: f64| {
        in_ar(bus as f64, num_channels).unwrap()
    });
    engine.register_fn("in_ar", |bus: i64, num_channels: i64| {
        in_ar(bus as f64, num_channels as f64).unwrap()
    });
    engine.register_fn("in_ar", |bus: NodeRef, num_channels: f64| {
        in_ar_n(bus, num_channels).unwrap()
    });
    engine.register_fn("in_ar", |bus: NodeRef, num_channels: i64| {
        in_ar_n(bus, num_channels as f64).unwrap()
    });
    engine.register_fn("replace_out_ar", |bus: f64, channels: Array| {
        replace_out_ar(bus, channels).unwrap()
    });
    engine.register_fn("replace_out_ar", |bus: NodeRef, channels: Array| {
        replace_out_ar_n(bus, channels).unwrap()
    });

    // Hardware audio input (line-in, microphone)
    engine.register_fn("sound_in", |num_channels: f64| {
        sound_in(num_channels).unwrap()
    });
    engine.register_fn("sound_in", |num_channels: i64| {
        sound_in(num_channels as f64).unwrap()
    });
    engine.register_fn("sound_in_channel", |channel: f64| {
        sound_in_channel(channel).unwrap()
    });
    engine.register_fn("sound_in_channel", |channel: i64| {
        sound_in_channel(channel as f64).unwrap()
    });
    engine.register_fn("sound_in_channel", |channel: NodeRef| {
        sound_in_channel_n(channel).unwrap()
    });

    // `sound_in_ar` is the manifest-style alias for `sound_in_channel`.
    // The SoundIn manifest entry is documentation-only (rates: ["builder"])
    // because SoundIn is a sclang pseudo-UGen; the SC server only knows
    // In + NumOutputBuses, which is what these helpers emit.
    engine.register_fn("sound_in_ar", |channel: f64| {
        sound_in_channel(channel).unwrap()
    });
    engine.register_fn("sound_in_ar", |channel: i64| {
        sound_in_channel(channel as f64).unwrap()
    });
    engine.register_fn("sound_in_ar", |channel: NodeRef| {
        sound_in_channel_n(channel).unwrap()
    });
    // Allow zero-arg call: `sound_in_ar()` reads channel 0.
    engine.register_fn("sound_in_ar", || sound_in_channel(0.0).unwrap());

    // Mix and utilities
    engine.register_fn("mix", |arr: Array| mix(arr).unwrap());
    engine.register_fn("sum", |arr: Array| mix(arr).unwrap()); // Alias for mix
    engine.register_fn("dup", |sig: NodeRef, count: i64| dup(sig, count).unwrap());
    engine.register_fn("channels", |sig: NodeRef, count: i64| {
        channels(sig, count).unwrap()
    });
    engine.register_fn("channel", |sig: NodeRef, index: i64| {
        channel(sig, index).unwrap()
    });
    engine.register_fn("detune_spread", |voices: i64, amount: f64| {
        detune_spread(voices, amount).unwrap()
    });

    // Array utilities
    engine.register_fn("zip", array_zip);

    // Envelopes
    engine.register_fn("env_gen", |gate: NodeRef, done: i64| {
        env_gen(gate, done).unwrap()
    });

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
        .register_fn(
            "build",
            |builder: EnvGenBuilder| -> std::result::Result<NodeRef, Box<rhai::EvalAltResult>> {
                EnvGenBuilder::build(builder).map_err(synthdef_error_to_eval)
            },
        );

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
        // Sustain methods
        .register_fn("sustain", EnvelopeBuilder::sustain)
        .register_fn("sustain", EnvelopeBuilder::sustain_f)
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
        .register_fn(
            "build",
            |builder: EnvelopeBuilder| -> std::result::Result<NodeRef, Box<rhai::EvalAltResult>> {
                EnvelopeBuilder::build(builder).map_err(synthdef_error_to_eval)
            },
        );

    // EnvGen with Env
    engine.register_fn(
        "env_gen",
        |env: Env,
         gate: NodeRef,
         level_scale: f64,
         level_bias: f64,
         time_scale: f64,
         done_action: f64| {
            env_gen_with_env(env, gate, level_scale, level_bias, time_scale, done_action).unwrap()
        },
    );
    engine.register_fn(
        "env_gen",
        |env: Env,
         gate: NodeRef,
         level_scale: NodeRef,
         level_bias: NodeRef,
         time_scale: NodeRef,
         done_action: NodeRef| {
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
