//! System SynthDefs for audio bus routing.
//!
//! This module builds the routing synthdefs (`system_link_audio`,
//! `system_link_audio_mono`, the `*_link_*` bus routers, the `a2k_adapter_1`
//! rate coercer and the `param_kr_modulate_<n>` summers) used for group bus
//! routing, metering and CV/param modulation.
//!
//! All synthdefs are built on the crate's [`GraphBuilderInner`] → [`GraphIR`]
//! → [`encode_synthdef`] pipeline — the same path user synthdefs and the
//! sample/SFZ system synthdefs take. The original hand-written SCgf v2 byte
//! encoders live in the test module as reference implementations; structural
//! equivalence between the two is asserted by the `equiv_*` tests. The two
//! `system_link_audio*` synthdefs have intentionally diverged from their
//! legacy encoders (de-click Lag smoothing + mute gate) and are covered by
//! structural `declick_*` tests instead.
//!
//! ## Signal Flow (system_link_audio)
//!
//! ```text
//! In.ar(inbus) → × Lag(amp) → balance(Lag(pan)) → × (1 − Lag(mute)) → Out.ar(outbus)
//!                                                          ↓
//!                                              Peak + Amplitude → SendTrig (at 20Hz)
//! ```
//!
//! All mixer controls (`amp`, `pan`, `mute`) are smoothed with a short
//! [`DECLICK_LAG_S`] `Lag.kr` so step changes arriving via `/n_set` fade
//! instead of clicking.
//!
//! ## SendTrig IDs
//!
//! - 0: peak_left
//! - 1: peak_right
//! - 2: rms_left
//! - 3: rms_right

use crate::{encode_synthdef, GraphBuilderInner, GraphIR, Input, NodeRef, Rate};

// BinaryOpUGen special indices used by the routing graphs.
const OP_ADD: i16 = 0;
const OP_SUB: i16 = 1;
const OP_MUL: i16 = 2;
const OP_MIN: i16 = 12;
const OP_MAX: i16 = 13;

/// Lag time (seconds) applied to the link-synth mixer controls (`amp`,
/// `pan`, `mute`). Converts step discontinuities from `/n_set` into ~30 ms
/// fades — long enough to de-click, short enough to feel immediate.
pub const DECLICK_LAG_S: f32 = 0.03;

/// Reference the Control UGen output for the parameter at `slot`.
///
/// Valid only after [`GraphBuilderInner::create_control_ugen`] has run, which
/// places the Control UGen at node index 0.
fn param(slot: u32) -> Input {
    Input::Node {
        node_id: 0,
        output_index: slot,
    }
}

/// Reference output `output_index` of a previously added node.
fn out_of(node: NodeRef, output_index: u32) -> Input {
    Input::Node {
        node_id: node.0,
        output_index,
    }
}

/// Reference the first (or only) output of a previously added node.
fn first(node: NodeRef) -> Input {
    out_of(node, 0)
}

/// Encode a finished builder as a single-synthdef SCgf v2 blob.
fn encode(name: &str, builder: GraphBuilderInner) -> Result<Vec<u8>, std::io::Error> {
    encode_synthdef(&GraphIR::from_builder(name.to_string(), builder))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
}

/// Build the shared `system_link_audio[_mono]` graph.
///
/// Both variants share the identical gain/pan/metering topology; the mono
/// variant inserts a `panned_left + panned_right` sum (no halving) and writes
/// a single channel to `outbus` instead of the stereo pair.
fn build_system_link_audio(name: &str, mono: bool) -> Result<Vec<u8>, std::io::Error> {
    let mut b = GraphBuilderInner::new();

    // Parameters: inbus=0, outbus=0, amp=1.0, pan=0.0, mute=0.0 (slots 0..=4).
    b.add_param("inbus".to_string(), vec![0.0], None);
    b.add_param("outbus".to_string(), vec![0.0], None);
    b.add_param("amp".to_string(), vec![1.0], None);
    b.add_param("pan".to_string(), vec![0.0], None);
    b.add_param("mute".to_string(), vec![0.0], None);
    b.create_control_ugen();

    // Constants:
    // 0.0 (SendTrig ID 0 / zero), 1.0 (ID 1 / unity), 2.0 (ID 2), 3.0 (ID 3),
    // 20.0 (meter Impulse rate), 0.01 (Amplitude attack), 0.1 (release),
    // DECLICK_LAG_S (control smoothing).
    for c in [0.0, 1.0, 2.0, 3.0, 20.0, 0.01, 0.1, DECLICK_LAG_S] {
        b.add_constant(c);
    }

    // In.ar(inbus, 2)
    let input = b.add_node("In".to_string(), Rate::Audio, vec![param(0)], 2, 0);

    // De-click: smooth every mixer control with a short Lag so step changes
    // arriving via /n_set (group amp fades, mute toggles) never produce an
    // audible discontinuity.
    let lag_amp = b.add_node(
        "Lag".to_string(),
        Rate::Control,
        vec![param(2), Input::Constant(DECLICK_LAG_S)],
        1,
        0,
    );

    // scaled = In * Lag(amp)
    let scaled_l = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![out_of(input, 0), first(lag_amp)],
        1,
        OP_MUL,
    );
    let scaled_r = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![out_of(input, 1), first(lag_amp)],
        1,
        OP_MUL,
    );

    // Linear balance law on the lagged pan:
    // left_gain = 1 - max(0, pan), right_gain = 1 + min(0, pan).
    let lag_pan = b.add_node(
        "Lag".to_string(),
        Rate::Control,
        vec![param(3), Input::Constant(DECLICK_LAG_S)],
        1,
        0,
    );
    let max_pan = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(0.0), first(lag_pan)],
        1,
        OP_MAX,
    );
    let left_gain = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(1.0), first(max_pan)],
        1,
        OP_SUB,
    );
    let min_pan = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(0.0), first(lag_pan)],
        1,
        OP_MIN,
    );
    let right_gain = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(1.0), first(min_pan)],
        1,
        OP_ADD,
    );

    let panned_l = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![first(scaled_l), first(left_gain)],
        1,
        OP_MUL,
    );
    let panned_r = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![first(scaled_r), first(right_gain)],
        1,
        OP_MUL,
    );

    // Mute gate: output × (1 - Lag(mute)). The runtime sets mute to 0/1 via
    // /n_set; the Lag turns the toggle into a short fade and — unlike the old
    // /n_run group pause — leaves the group's children (and FX tails) running.
    let lag_mute = b.add_node(
        "Lag".to_string(),
        Rate::Control,
        vec![param(4), Input::Constant(DECLICK_LAG_S)],
        1,
        0,
    );
    let mute_gate = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![Input::Constant(1.0), first(lag_mute)],
        1,
        OP_SUB,
    );
    let gated_l = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![first(panned_l), first(mute_gate)],
        1,
        OP_MUL,
    );
    let gated_r = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Audio,
        vec![first(panned_r), first(mute_gate)],
        1,
        OP_MUL,
    );

    if mono {
        // mono_sum = gated_left + gated_right.
        // No halving: unity-gain mono mixdown. True-stereo content runs ~6 dB hot.
        let mono_sum = b.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Audio,
            vec![first(gated_l), first(gated_r)],
            1,
            OP_ADD,
        );
        b.add_node(
            "Out".to_string(),
            Rate::Audio,
            vec![param(1), first(mono_sum)],
            0,
            0,
        );
    } else {
        b.add_node(
            "Out".to_string(),
            Rate::Audio,
            vec![param(1), first(gated_l), first(gated_r)],
            0,
            0,
        );
    }

    // Metering taps the internal (pre-mixdown, post-mute) stereo signal in
    // both variants, so meters fall to zero while the group is muted.
    let impulse = b.add_node(
        "Impulse".to_string(),
        Rate::Control,
        vec![Input::Constant(20.0), Input::Constant(0.0)],
        1,
        0,
    );
    let peak_l = b.add_node(
        "Peak".to_string(),
        Rate::Control,
        vec![first(gated_l), first(impulse)],
        1,
        0,
    );
    let peak_r = b.add_node(
        "Peak".to_string(),
        Rate::Control,
        vec![first(gated_r), first(impulse)],
        1,
        0,
    );
    let amp_l = b.add_node(
        "Amplitude".to_string(),
        Rate::Control,
        vec![first(gated_l), Input::Constant(0.01), Input::Constant(0.1)],
        1,
        0,
    );
    let amp_r = b.add_node(
        "Amplitude".to_string(),
        Rate::Control,
        vec![first(gated_r), Input::Constant(0.01), Input::Constant(0.1)],
        1,
        0,
    );

    // SendTrig IDs 0..3: peak_l, peak_r, rms_l, rms_r.
    for (id, meter) in [(0.0, peak_l), (1.0, peak_r), (2.0, amp_l), (3.0, amp_r)] {
        b.add_node(
            "SendTrig".to_string(),
            Rate::Control,
            vec![first(impulse), Input::Constant(id), first(meter)],
            0,
            0,
        );
    }

    encode(name, b)
}

/// Create the system_link_audio synthdef bytes.
///
/// This synthdef routes audio from one bus to another with amplitude control,
/// stereo balance (pan), and metering. Used for group bus routing in the mixer.
///
/// # Parameters
///
/// - `inbus`: Input bus number (default: 0)
/// - `outbus`: Output bus number (default: 0)
/// - `amp`: Amplitude multiplier (default: 1.0)
/// - `pan`: Stereo balance, -1=left, 0=center, 1=right (default: 0.0)
/// - `mute`: Mute gate, 0=audible, 1=muted (default: 0.0)
///
/// `amp`, `pan` and `mute` are smoothed with `Lag.kr(_, DECLICK_LAG_S)`, so
/// step changes via `/n_set` fade over ~30 ms instead of clicking. Mute is
/// implemented as `signal × (1 - Lag(mute))` — children keep running, so FX
/// tails survive a mute/unmute cycle.
///
/// # Metering
///
/// The synthdef sends meter data via SendTrig at 20Hz:
/// - Trigger ID 0: Left channel peak
/// - Trigger ID 1: Right channel peak
/// - Trigger ID 2: Left channel RMS (via Amplitude UGen)
/// - Trigger ID 3: Right channel RMS (via Amplitude UGen)
pub fn create_system_link_audio_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_system_link_audio("system_link_audio", false)
}

/// Create the system_link_audio_mono synthdef bytes.
///
/// Mono-mixdown variant of [`create_system_link_audio_bytes`]: the internal
/// graph (gain, pan, metering) is identical and operates on a 2-channel bus
/// exactly as in the stereo version, but the final `Out.ar` writes a single
/// channel = `panned_left + panned_right` (sum, no halving) to a single
/// hardware bus. Used for `group.output(N)` with one hw channel.
///
/// True-stereo content summed without halving will be ~6 dB hotter than the
/// stereo version — this is intentional: mono synthdefs run at unity gain and
/// the user accepts the headroom hit for true-stereo program material.
///
/// # Parameters
///
/// - `inbus`: 2-channel input bus (default: 0)
/// - `outbus`: 1-channel output bus (default: 0)
/// - `amp`: Amplitude multiplier (default: 1.0)
/// - `pan`: Stereo balance applied *before* the L+R sum, so the user can
///   bias the mixdown toward one side of the source (default: 0.0)
/// - `mute`: Mute gate, 0=audible, 1=muted (default: 0.0)
///
/// `amp`, `pan` and `mute` are Lag-smoothed exactly as in the stereo variant.
///
/// # Metering
///
/// Identical to the stereo synthdef — Peak/Amplitude UGens tap the internal
/// 2-channel signal pre-mixdown (post mute-gate), so the SendTrig payload at
/// trigger IDs 0..3 carries the same data the dashboard already understands.
/// Mixdown to mono only affects the final `Out.ar`.
pub fn create_system_link_audio_mono_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_system_link_audio("system_link_audio_mono", true)
}

/// Build a simple `In → Out` bus-link graph with `in_bus`/`out_bus` params.
///
/// `channels` is the number of channels read from `in_bus`. When
/// `dup_mono_to_stereo` is set (only valid for `channels == 1`), the single
/// input channel is written twice, duplicating mono into a stereo pair.
fn build_bus_link(
    name: &str,
    rate: Rate,
    channels: u32,
    dup_mono_to_stereo: bool,
) -> Result<Vec<u8>, std::io::Error> {
    let mut b = GraphBuilderInner::new();
    b.add_param("in_bus".to_string(), vec![0.0], None);
    b.add_param("out_bus".to_string(), vec![0.0], None);
    b.create_control_ugen();

    let input = b.add_node("In".to_string(), rate, vec![param(0)], channels, 0);

    let mut out_inputs = vec![param(1)];
    for ch in 0..channels {
        out_inputs.push(out_of(input, ch));
    }
    if dup_mono_to_stereo {
        debug_assert_eq!(channels, 1);
        out_inputs.push(out_of(input, 0));
    }
    b.add_node("Out".to_string(), rate, out_inputs, 0, 0);

    encode(name, b)
}

/// Create the `port_to_group_link_1` synthdef bytes.
///
/// Routes one mono audio bus into a stereo destination bus by duplicating
/// the mono signal across both output channels:
///
/// ```supercollider
/// SynthDef("port_to_group_link_1", { |in_bus=0, out_bus=0|
///     Out.ar(out_bus, In.ar(in_bus, 1).dup)
/// })
/// ```
///
/// Used by `RoutesHandler::finalize` to tap a voice's mono output port and
/// mix it into a group's stereo audio bus.
pub fn create_port_to_group_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_bus_link("port_to_group_link_1", Rate::Audio, 1, true)
}

/// Create the `port_to_group_link_2` synthdef bytes.
///
/// Routes a stereo audio bus into a stereo destination bus, passing through
/// both channels unchanged:
///
/// ```supercollider
/// SynthDef("port_to_group_link_2", { |in_bus=0, out_bus=0|
///     Out.ar(out_bus, In.ar(in_bus, 2))
/// })
/// ```
///
/// Used by `RoutesHandler::finalize` to tap a voice's stereo output port
/// and mix it into a group's stereo audio bus.
pub fn create_port_to_group_link_2_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_bus_link("port_to_group_link_2", Rate::Audio, 2, false)
}

/// Create the `input_link_1` synthdef bytes.
///
/// Mono input router. Reads a mono audio bus and writes the signal at unity
/// gain to a mono destination bus:
///
/// ```supercollider
/// SynthDef("input_link_1", { |in_bus=0, out_bus=0|
///     Out.ar(out_bus, In.ar(in_bus, 1))
/// })
/// ```
///
/// Used by the named-inputs dispatcher to wire a source bus into a voice's
/// mono named-input bus. Fan-in summing is free — multiple `input_link_*`
/// synths writing the same `out_bus` sum at the bus.
pub fn create_input_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_bus_link("input_link_1", Rate::Audio, 1, false)
}

/// Create the `input_link_2` synthdef bytes.
///
/// Stereo input router. Reads a stereo audio bus and writes both channels at
/// unity gain to a stereo destination bus:
///
/// ```supercollider
/// SynthDef("input_link_2", { |in_bus=0, out_bus=0|
///     Out.ar(out_bus, In.ar(in_bus, 2))
/// })
/// ```
///
/// Used by the named-inputs dispatcher to wire a source bus into a voice's
/// stereo named-input bus. Fan-in summing is free — multiple `input_link_*`
/// synths writing the same `out_bus` sum at the bus.
pub fn create_input_link_2_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_bus_link("input_link_2", Rate::Audio, 2, false)
}

/// Create the `port_tr_to_param_link_1` synthdef bytes.
///
/// One-to-one trigger forwarding from a Tr-rate source bus to a kr destination
/// bus, used by `RoutesHandler::finalize` (B2.c) to wire a voice's Tr port onto
/// a target voice's param. SC has no separate `Out.tr` UGen — Tr ports already
/// ride the `Out.kr` codegen path, so the source's trigger sits on a control
/// bus as a kr-rate signal whose single-sample edges land on kr-block
/// boundaries. `In.kr` reads it back unchanged; `Out.kr` forwards it,
/// preserving the edge alignment without the scale/offset shaping that
/// `param_kr_modulate_<n>` would impose. Equivalent to:
///
/// ```supercollider
/// SynthDef("port_tr_to_param_link_1", { |in_bus=0, out_bus=0|
///     Out.kr(out_bus, In.kr(in_bus, 1))
/// })
/// ```
///
/// Triggers route to params, not group/main buses, so there is intentionally
/// no `port_tr_to_group_*` variant — group destinations are an audio-rate
/// concept.
pub fn create_port_tr_to_param_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
    build_bus_link("port_tr_to_param_link_1", Rate::Control, 1, false)
}

/// Create the `a2k_adapter_1` synthdef bytes.
///
/// One-channel ar→kr adapter used by `RoutesHandler::finalize_params`
/// when a `.to_param_audio()` route demands rate coercion: it reads a single
/// audio-rate bus, samples it once per kr cycle via SuperCollider's `A2K`
/// UGen, and writes the kr signal to a control bus. The control bus then
/// feeds the same `param_kr_modulate_<n>` summer infrastructure used by
/// pure-kr routes, so per-source `.scale()` / `.offset()` shaping survives
/// the coercion step. Equivalent to:
///
/// ```supercollider
/// SynthDef("a2k_adapter_1", { |in_bus=0, out_bus=0|
///     Out.kr(out_bus, A2K.kr(In.ar(in_bus, 1)))
/// })
/// ```
///
/// Mono only — kr ports are mono by convention, and multi-channel adapters
/// aren't needed for the param-modulation case the verb targets.
pub fn create_a2k_adapter_1_bytes() -> Result<Vec<u8>, std::io::Error> {
    let mut b = GraphBuilderInner::new();
    b.add_param("in_bus".to_string(), vec![0.0], None);
    b.add_param("out_bus".to_string(), vec![0.0], None);
    b.create_control_ugen();

    let input = b.add_node("In".to_string(), Rate::Audio, vec![param(0)], 1, 0);
    let a2k = b.add_node("A2K".to_string(), Rate::Control, vec![first(input)], 1, 0);
    b.add_node(
        "Out".to_string(),
        Rate::Control,
        vec![param(1), first(a2k)],
        0,
        0,
    );

    encode("a2k_adapter_1", b)
}

/// Maximum number of source kr signals supported by `param_kr_modulate_<n>`.
///
/// Sets the upper bound that [`create_param_kr_modulate_n_bytes`] will accept
/// and caps how many `modulate_by` sources can target one `(voice, param)`
/// pair without truncation. Eight is a comfortable headroom for typical
/// modular patches (env + lfo + macro + offset + …) without bloating the
/// synthdef set.
pub const PARAM_KR_MODULATE_MAX: usize = 8;

/// Per-source parameter suffixes for `param_kr_modulate_<n>`.
const PORT_LETTERS: [char; PARAM_KR_MODULATE_MAX] = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];

/// Create the `param_kr_modulate_<n>` synthdef bytes for the given source count.
///
/// The synthdef adds a `baseline` control to `n` per-source affine mixers
/// (`scale_<i> * In.kr(in_<i>, 1) + offset_<i>`) and writes the result onto a
/// single intermediate control bus. Used by `RoutesHandler::finalize_params`
/// for both the SET and BEND paths of multi-output v2: SET pins
/// `baseline=0` so the source signal carries straight through (with optional
/// scale/offset shaping), BEND lets the user's `set_param` value live in
/// `baseline` while modulators bend around it. Equivalent to:
///
/// ```supercollider
/// SynthDef("param_kr_modulate_<n>", { |baseline=0,
///                                       in_a=0, scale_a=1, offset_a=0,
///                                       in_b=0, scale_b=1, offset_b=0,
///                                       ...,
///                                       out_bus=0|
///     Out.kr(out_bus, baseline
///                       + scale_a * In.kr(in_a, 1) + offset_a
///                       + scale_b * In.kr(in_b, 1) + offset_b
///                       + ...)
/// })
/// ```
///
/// `n` must be in `1..=PARAM_KR_MODULATE_MAX`. At `n=1` the synthdef is still
/// a summer (`baseline + scale_a * In.kr(in_a, 1) + offset_a`) — that's the
/// whole point of unified routing: the summer is *always* present so SET can
/// fall through with `baseline=0, scale=1, offset=0` while BEND piggybacks
/// the user's `set_param` value as `baseline`. The `n` per-source parameters
/// are named `in_a`, `scale_a`, `offset_a`, `in_b`, `scale_b`, `offset_b`,
/// … in declaration order; defaults are `in_<i>=0`, `scale_<i>=1`,
/// `offset_<i>=0`. The destination bus parameter is `out_bus`. Allocation
/// and teardown of the intermediate bus + summer node is owned by
/// `crates/vibelang-core::handlers::routes::RoutesHandler::finalize_params`.
pub fn create_param_kr_modulate_n_bytes(n: usize) -> Result<Vec<u8>, std::io::Error> {
    assert!(
        (1..=PARAM_KR_MODULATE_MAX).contains(&n),
        "param_kr_modulate_<n> only supports n in 1..={} (got {})",
        PARAM_KR_MODULATE_MAX,
        n
    );

    let mut b = GraphBuilderInner::new();

    // Parameter layout (all kr controls):
    //   slot 0          : baseline   (default 0.0)
    //   slot 1 + 3*i    : in_<i>     (default 0.0)
    //   slot 2 + 3*i    : scale_<i>  (default 1.0)
    //   slot 3 + 3*i    : offset_<i> (default 0.0)
    //   slot 1 + 3*n    : out_bus    (default 0.0)
    b.add_param("baseline".to_string(), vec![0.0], None);
    for &letter in PORT_LETTERS.iter().take(n) {
        b.add_param(format!("in_{}", letter), vec![0.0], None);
        b.add_param(format!("scale_{}", letter), vec![1.0], None);
        b.add_param(format!("offset_{}", letter), vec![0.0], None);
    }
    b.add_param("out_bus".to_string(), vec![0.0], None);
    b.create_control_ugen();

    // Per-source In.kr(in_<i>, 1) readers.
    let ins: Vec<NodeRef> = (0..n)
        .map(|i| {
            b.add_node(
                "In".to_string(),
                Rate::Control,
                vec![param((1 + 3 * i) as u32)],
                1,
                0,
            )
        })
        .collect();

    // scale_<i> * In.kr_<i>
    let muls: Vec<NodeRef> = (0..n)
        .map(|i| {
            b.add_node(
                "BinaryOpUGen".to_string(),
                Rate::Control,
                vec![param((2 + 3 * i) as u32), first(ins[i])],
                1,
                OP_MUL,
            )
        })
        .collect();

    // (scale_<i> * In.kr_<i>) + offset_<i>
    let shaped: Vec<NodeRef> = (0..n)
        .map(|i| {
            b.add_node(
                "BinaryOpUGen".to_string(),
                Rate::Control,
                vec![first(muls[i]), param((3 + 3 * i) as u32)],
                1,
                OP_ADD,
            )
        })
        .collect();

    // Cumulative sum: sum_0 = baseline + shaped_0; sum_k = sum_{k-1} + shaped_k.
    let mut sum = b.add_node(
        "BinaryOpUGen".to_string(),
        Rate::Control,
        vec![param(0), first(shaped[0])],
        1,
        OP_ADD,
    );
    for &next in shaped.iter().skip(1) {
        sum = b.add_node(
            "BinaryOpUGen".to_string(),
            Rate::Control,
            vec![first(sum), first(next)],
            1,
            OP_ADD,
        );
    }

    // Out.kr(out_bus, final_sum)
    b.add_node(
        "Out".to_string(),
        Rate::Control,
        vec![param((1 + 3 * n) as u32), first(sum)],
        0,
        0,
    );

    encode(&format!("param_kr_modulate_{}", n), b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original hand-written SCgf v2 byte encoders, kept verbatim as the
    /// reference implementations for the `equiv_*` structural-equivalence
    /// tests below. The production functions of the same names (in the parent
    /// module) build the identical graphs via GraphIR/`encode_synthdef`; the
    /// tests decode both blobs and compare them up to UGen reordering and
    /// constant-table permutation. Do not "fix" or modernize these — their
    /// value is being a frozen, independently-written encoding of the same
    /// synthdefs.
    ///
    /// The legacy encoders for `system_link_audio` and
    /// `system_link_audio_mono` were removed when the production graphs
    /// intentionally diverged (de-click Lag smoothing on amp/pan and a lagged
    /// `mute` gate); those two synthdefs are now verified by the structural
    /// `declick_*` tests instead of byte-level equivalence.
    mod legacy {
        use std::io::Write;

        /// Write a UGen input reference to the buffer.
        fn write_ugen_input(
            buf: &mut Vec<u8>,
            ugen_idx: i32,
            output_idx: i32,
        ) -> std::io::Result<()> {
            buf.write_all(&ugen_idx.to_be_bytes())?;
            buf.write_all(&output_idx.to_be_bytes())?;
            Ok(())
        }

        pub fn create_port_to_group_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"port_to_group_link_1";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            // 0: Control (2 control outputs: in_bus, out_bus)
            // 1: In.ar (1 audio output, mono, reads in_bus)
            // 2: Out.ar (writes In[0] twice → stereo dup at out_bus)
            buf.write_all(&3i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?; // 0 inputs
            buf.write_all(&2i32.to_be_bytes())?; // 2 outputs
            buf.write_all(&0i16.to_be_bytes())?; // special index
            buf.push(1); // output 0 rate
            buf.push(1); // output 1 rate

            // UGen 1: In.ar(in_bus, 1) → mono
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(2); // audio rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(2); // output rate

            // UGen 2: Out.ar(out_bus, mono, mono) → mono duplicated to stereo
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(2); // audio rate
            buf.write_all(&3i32.to_be_bytes())?; // 3 inputs
            buf.write_all(&0i32.to_be_bytes())?; // 0 outputs
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 1, 0)?; // In[0] (left channel)
            write_ugen_input(&mut buf, 1, 0)?; // In[0] again (right channel — dup)

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_port_to_group_link_2_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"port_to_group_link_2";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            // 0: Control (2 control outputs: in_bus, out_bus)
            // 1: In.ar (2 audio outputs: left, right; reads in_bus)
            // 2: Out.ar (writes left + right → stereo passthrough at out_bus)
            buf.write_all(&3i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            buf.push(1);
            buf.push(1);

            // UGen 1: In.ar(in_bus, 2) → stereo
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(2); // audio rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&2i32.to_be_bytes())?; // 2 outputs (stereo)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(2); // output 0 rate
            buf.push(2); // output 1 rate

            // UGen 2: Out.ar(out_bus, left, right)
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(2); // audio rate
            buf.write_all(&3i32.to_be_bytes())?;
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 1, 0)?; // In[0] = left
            write_ugen_input(&mut buf, 1, 1)?; // In[1] = right

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_input_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"input_link_1";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            // 0: Control (2 control outputs: in_bus, out_bus)
            // 1: In.ar (1 audio output, mono, reads in_bus)
            // 2: Out.ar (writes In[0] → mono passthrough at out_bus)
            buf.write_all(&3i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?; // 0 inputs
            buf.write_all(&2i32.to_be_bytes())?; // 2 outputs
            buf.write_all(&0i16.to_be_bytes())?; // special index
            buf.push(1); // output 0 rate
            buf.push(1); // output 1 rate

            // UGen 1: In.ar(in_bus, 1) → mono
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(2); // audio rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(2); // output rate

            // UGen 2: Out.ar(out_bus, mono) → mono passthrough
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(2); // audio rate
            buf.write_all(&2i32.to_be_bytes())?; // 2 inputs (bus + 1 channel)
            buf.write_all(&0i32.to_be_bytes())?; // 0 outputs
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 1, 0)?; // In[0] (mono channel)

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_input_link_2_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"input_link_2";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            // 0: Control (2 control outputs: in_bus, out_bus)
            // 1: In.ar (2 audio outputs: left, right; reads in_bus)
            // 2: Out.ar (writes left + right → stereo passthrough at out_bus)
            buf.write_all(&3i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            buf.push(1);
            buf.push(1);

            // UGen 1: In.ar(in_bus, 2) → stereo
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(2); // audio rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&2i32.to_be_bytes())?; // 2 outputs (stereo)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(2); // output 0 rate
            buf.push(2); // output 1 rate

            // UGen 2: Out.ar(out_bus, left, right)
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(2); // audio rate
            buf.write_all(&3i32.to_be_bytes())?;
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 1, 0)?; // In[0] = left
            write_ugen_input(&mut buf, 1, 1)?; // In[1] = right

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_port_tr_to_param_link_1_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"port_tr_to_param_link_1";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            //   0: Control (kr) — 2 outputs: in_bus, out_bus
            //   1: In.kr(in_bus, 1) — 1 mono kr output
            //   2: Out.kr(out_bus, In[0])
            buf.write_all(&3i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            buf.push(1); // output 0 rate
            buf.push(1); // output 1 rate

            // UGen 1: In.kr(in_bus, 1) → mono kr (carries the trigger edge unchanged)
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(1); // control rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono kr)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(1); // output rate

            // UGen 2: Out.kr(out_bus, In[0])
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(1); // control rate
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0i32.to_be_bytes())?;
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 1, 0)?; // In[0]

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_a2k_adapter_1_bytes() -> Result<Vec<u8>, std::io::Error> {
            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name
            let name = b"a2k_adapter_1";
            buf.push(name.len() as u8);
            buf.write_all(name)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameters: in_bus=0, out_bus=0
            buf.write_all(&2i32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;
            buf.write_all(&0.0f32.to_be_bytes())?;

            buf.write_all(&2i32.to_be_bytes())?;
            let in_bus_name = b"in_bus";
            buf.push(in_bus_name.len() as u8);
            buf.write_all(in_bus_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&1i32.to_be_bytes())?;

            // UGens:
            //   0: Control (kr) — 2 outputs: in_bus, out_bus
            //   1: In.ar(in_bus, 1) — 1 mono ar output
            //   2: A2K.kr(In[0]) — 1 mono kr output (samples ar once per kr cycle)
            //   3: Out.kr(out_bus, A2K[0])
            buf.write_all(&4i32.to_be_bytes())?;

            // UGen 0: Control
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?; // 0 inputs
            buf.write_all(&2i32.to_be_bytes())?; // 2 outputs
            buf.write_all(&0i16.to_be_bytes())?; // special index
            buf.push(1); // output 0 rate (in_bus)
            buf.push(1); // output 1 rate (out_bus)

            // UGen 1: In.ar(in_bus, 1) → mono ar
            let in_name = b"In";
            buf.push(in_name.len() as u8);
            buf.write_all(in_name)?;
            buf.push(2); // audio rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
            buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono ar)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = in_bus
            buf.push(2); // output rate: audio

            // UGen 2: A2K.kr(In[0]) — downsample ar to kr
            let a2k_name = b"A2K";
            buf.push(a2k_name.len() as u8);
            buf.write_all(a2k_name)?;
            buf.push(1); // control rate
            buf.write_all(&1i32.to_be_bytes())?; // 1 input (the ar signal)
            buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono kr)
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 1, 0)?; // In[0] (mono ar)
            buf.push(1); // output rate: control

            // UGen 3: Out.kr(out_bus, A2K[0])
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(1); // control rate
            buf.write_all(&2i32.to_be_bytes())?; // 2 inputs (bus index + signal)
            buf.write_all(&0i32.to_be_bytes())?; // 0 outputs
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, 1)?; // Control[1] = out_bus
            write_ugen_input(&mut buf, 2, 0)?; // A2K[0]

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }

        pub fn create_param_kr_modulate_n_bytes(n: usize) -> Result<Vec<u8>, std::io::Error> {
            assert!(
                (1..=super::PARAM_KR_MODULATE_MAX).contains(&n),
                "param_kr_modulate_<n> only supports n in 1..={} (got {})",
                super::PARAM_KR_MODULATE_MAX,
                n
            );

            let mut buf = Vec::new();

            // File header
            buf.write_all(b"SCgf")?;
            buf.write_all(&2i32.to_be_bytes())?; // version 2
            buf.write_all(&1i16.to_be_bytes())?; // num synthdefs

            // Name: "param_kr_modulate_<n>"
            let name_string = format!("param_kr_modulate_{}", n);
            let name_bytes = name_string.as_bytes();
            buf.push(name_bytes.len() as u8);
            buf.write_all(name_bytes)?;

            // No constants
            buf.write_all(&0i32.to_be_bytes())?;

            // Parameter layout (all kr controls):
            //   index 0          : baseline   (default 0.0)
            //   index 1 + 3*i    : in_<i>     (default 0.0)
            //   index 2 + 3*i    : scale_<i>  (default 1.0)
            //   index 3 + 3*i    : offset_<i> (default 0.0)
            //   index 1 + 3*n    : out_bus    (default 0.0)
            // Total params: 1 + 3*n + 1 = 3n + 2.
            let num_params = (3 * n + 2) as i32;
            buf.write_all(&num_params.to_be_bytes())?;
            // baseline default
            buf.write_all(&0.0f32.to_be_bytes())?;
            // per-source defaults: in_<i>=0, scale_<i>=1, offset_<i>=0
            for _ in 0..n {
                buf.write_all(&0.0f32.to_be_bytes())?; // in_<i>
                buf.write_all(&1.0f32.to_be_bytes())?; // scale_<i>
                buf.write_all(&0.0f32.to_be_bytes())?; // offset_<i>
            }
            // out_bus default
            buf.write_all(&0.0f32.to_be_bytes())?;

            // Param names
            let port_letters = [b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'];
            buf.write_all(&num_params.to_be_bytes())?;
            let baseline_name = b"baseline";
            buf.push(baseline_name.len() as u8);
            buf.write_all(baseline_name)?;
            buf.write_all(&0i32.to_be_bytes())?;
            for i in 0..n {
                let in_name = [b'i', b'n', b'_', port_letters[i]];
                buf.push(in_name.len() as u8);
                buf.write_all(&in_name)?;
                buf.write_all(&((1 + 3 * i) as i32).to_be_bytes())?;
                let scale_name = [b's', b'c', b'a', b'l', b'e', b'_', port_letters[i]];
                buf.push(scale_name.len() as u8);
                buf.write_all(&scale_name)?;
                buf.write_all(&((2 + 3 * i) as i32).to_be_bytes())?;
                let offset_name = [b'o', b'f', b'f', b's', b'e', b't', b'_', port_letters[i]];
                buf.push(offset_name.len() as u8);
                buf.write_all(&offset_name)?;
                buf.write_all(&((3 + 3 * i) as i32).to_be_bytes())?;
            }
            let out_bus_name = b"out_bus";
            buf.push(out_bus_name.len() as u8);
            buf.write_all(out_bus_name)?;
            buf.write_all(&((1 + 3 * n) as i32).to_be_bytes())?;

            // UGen layout (all kr-rate):
            //   0              : Control with (3n + 2) outputs
            //   1 ..= n        : In.kr(in_<i>, 1) — one per source
            //   n+1 ..= 2n     : BinaryOpUGen mul — scale_<i> * In.kr_<i>
            //   2n+1 ..= 3n    : BinaryOpUGen add — (scale_<i> * In.kr_<i>) + offset_<i>
            //   3n+1 ..= 4n    : BinaryOpUGen add — cumulative sum
            //   4n+1           : Out.kr(out_bus, sum_{n-1})
            let num_ugens = (4 * n + 2) as i32;
            buf.write_all(&num_ugens.to_be_bytes())?;

            // UGen 0: Control (kr) with (3n + 2) outputs
            let control_name = b"Control";
            buf.push(control_name.len() as u8);
            buf.write_all(control_name)?;
            buf.push(1); // control rate
            buf.write_all(&0i32.to_be_bytes())?; // 0 inputs
            buf.write_all(&((3 * n + 2) as i32).to_be_bytes())?; // 3n+2 outputs
            buf.write_all(&0i16.to_be_bytes())?; // special index
            for _ in 0..(3 * n + 2) {
                buf.push(1); // each output kr-rate
            }

            // UGens 1..=n: In.kr(in_<letter>, 1) — one per source
            let in_name = b"In";
            for i in 0..n {
                buf.push(in_name.len() as u8);
                buf.write_all(in_name)?;
                buf.push(1); // control rate
                buf.write_all(&1i32.to_be_bytes())?; // 1 input (the bus)
                buf.write_all(&1i32.to_be_bytes())?; // 1 output (mono kr)
                buf.write_all(&0i16.to_be_bytes())?;
                // Control[1 + 3*i] = in_<letter>
                write_ugen_input(&mut buf, 0, (1 + 3 * i) as i32)?;
                buf.push(1); // output rate
            }

            let binop_name = b"BinaryOpUGen";

            // UGens n+1..=2n: scale_<i> * In.kr_<i>  (special index 2 = mul)
            for i in 0..n {
                buf.push(binop_name.len() as u8);
                buf.write_all(binop_name)?;
                buf.push(1); // control rate
                buf.write_all(&2i32.to_be_bytes())?;
                buf.write_all(&1i32.to_be_bytes())?;
                buf.write_all(&2i16.to_be_bytes())?; // special index 2 = mul
                write_ugen_input(&mut buf, 0, (2 + 3 * i) as i32)?; // Control[2+3i] = scale_<i>
                write_ugen_input(&mut buf, (1 + i) as i32, 0)?; // UGen[1+i] = In.kr_<i>
                buf.push(1); // output rate
            }

            // UGens 2n+1..=3n: (scale_<i> * In.kr_<i>) + offset_<i>  (special index 0 = add)
            for i in 0..n {
                buf.push(binop_name.len() as u8);
                buf.write_all(binop_name)?;
                buf.push(1); // control rate
                buf.write_all(&2i32.to_be_bytes())?;
                buf.write_all(&1i32.to_be_bytes())?;
                buf.write_all(&0i16.to_be_bytes())?; // special index 0 = add
                write_ugen_input(&mut buf, (n as i32) + 1 + (i as i32), 0)?; // mul output
                write_ugen_input(&mut buf, 0, (3 + 3 * i) as i32)?; // Control[3+3i] = offset_<i>
                buf.push(1); // output rate
            }

            // UGens 3n+1..=4n: cumulative BinaryOpUGen add (special index 0).
            for k in 0..n {
                buf.push(binop_name.len() as u8);
                buf.write_all(binop_name)?;
                buf.push(1); // control rate
                buf.write_all(&2i32.to_be_bytes())?;
                buf.write_all(&1i32.to_be_bytes())?;
                buf.write_all(&0i16.to_be_bytes())?; // special index 0 = add
                if k == 0 {
                    write_ugen_input(&mut buf, 0, 0)?; // Control[0] = baseline
                    write_ugen_input(&mut buf, (2 * n) as i32 + 1, 0)?; // first scaled_offset
                } else {
                    // prev cumulative sum: at UGen index 3n + k
                    write_ugen_input(&mut buf, (3 * n) as i32 + (k as i32), 0)?;
                    // next scaled_offset: at UGen index 2n + 1 + k
                    write_ugen_input(&mut buf, (2 * n) as i32 + 1 + (k as i32), 0)?;
                }
                buf.push(1); // output rate
            }

            // UGen 4n+1: Out.kr(out_bus, final_sum)
            let out_name = b"Out";
            buf.push(out_name.len() as u8);
            buf.write_all(out_name)?;
            buf.push(1); // control rate
            buf.write_all(&2i32.to_be_bytes())?; // 2 inputs (bus index + signal)
            buf.write_all(&0i32.to_be_bytes())?; // 0 outputs
            buf.write_all(&0i16.to_be_bytes())?;
            write_ugen_input(&mut buf, 0, (1 + 3 * n) as i32)?; // Control[1+3n] = out_bus
            write_ugen_input(&mut buf, (4 * n) as i32, 0)?; // last cumulative-sum output

            // No variants
            buf.write_all(&0i16.to_be_bytes())?;

            Ok(buf)
        }
    }

    // ---------------------------------------------------------------------
    // SCgf v2 decoder + canonical structural comparison
    // ---------------------------------------------------------------------

    /// A decoded UGen input edge: either a constant *value* (resolved through
    /// the constant table, so table permutation doesn't matter) or an
    /// `(ugen index, output index)` edge.
    #[derive(Debug, Clone, PartialEq)]
    enum DecodedInput {
        Const(f32),
        Node(usize, usize),
    }

    #[derive(Debug, Clone)]
    struct DecodedUGen {
        class: String,
        rate: u8,
        special: i16,
        inputs: Vec<DecodedInput>,
        output_rates: Vec<u8>,
    }

    #[derive(Debug, Clone)]
    struct DecodedSynthDef {
        name: String,
        constants: Vec<f32>,
        param_defaults: Vec<f32>,
        param_names: Vec<(String, usize)>,
        ugens: Vec<DecodedUGen>,
    }

    struct Cursor<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        fn new(bytes: &'a [u8]) -> Self {
            Self { bytes, pos: 0 }
        }

        fn take(&mut self, n: usize) -> &'a [u8] {
            assert!(
                self.pos + n <= self.bytes.len(),
                "decode overrun at offset {} (+{}), len {}",
                self.pos,
                n,
                self.bytes.len()
            );
            let s = &self.bytes[self.pos..self.pos + n];
            self.pos += n;
            s
        }

        fn u8(&mut self) -> u8 {
            self.take(1)[0]
        }

        fn i16(&mut self) -> i16 {
            i16::from_be_bytes(self.take(2).try_into().unwrap())
        }

        fn i32(&mut self) -> i32 {
            i32::from_be_bytes(self.take(4).try_into().unwrap())
        }

        fn f32(&mut self) -> f32 {
            f32::from_be_bytes(self.take(4).try_into().unwrap())
        }

        fn pstring(&mut self) -> String {
            let len = self.u8() as usize;
            String::from_utf8(self.take(len).to_vec()).expect("non-utf8 pstring")
        }
    }

    /// Decode a single-synthdef SCgf v2 blob into a structural representation.
    fn decode_synthdef(bytes: &[u8]) -> DecodedSynthDef {
        let mut c = Cursor::new(bytes);

        assert_eq!(c.take(4), b"SCgf", "bad magic");
        assert_eq!(c.i32(), 2, "expected SCgf version 2");
        assert_eq!(c.i16(), 1, "expected exactly one synthdef");

        let name = c.pstring();

        let num_constants = c.i32() as usize;
        let constants: Vec<f32> = (0..num_constants).map(|_| c.f32()).collect();

        let num_param_slots = c.i32() as usize;
        let param_defaults: Vec<f32> = (0..num_param_slots).map(|_| c.f32()).collect();

        let num_param_names = c.i32() as usize;
        let param_names: Vec<(String, usize)> = (0..num_param_names)
            .map(|_| {
                let n = c.pstring();
                let idx = c.i32() as usize;
                (n, idx)
            })
            .collect();

        let num_ugens = c.i32() as usize;
        let mut ugens = Vec::with_capacity(num_ugens);
        for _ in 0..num_ugens {
            let class = c.pstring();
            let rate = c.u8();
            let num_inputs = c.i32() as usize;
            let num_outputs = c.i32() as usize;
            let special = c.i16();
            let inputs = (0..num_inputs)
                .map(|_| {
                    let src = c.i32();
                    let idx = c.i32();
                    if src == -1 {
                        let ci = idx as usize;
                        assert!(ci < constants.len(), "constant index out of range");
                        DecodedInput::Const(constants[ci])
                    } else {
                        DecodedInput::Node(src as usize, idx as usize)
                    }
                })
                .collect();
            let output_rates = (0..num_outputs).map(|_| c.u8()).collect();
            ugens.push(DecodedUGen {
                class,
                rate,
                special,
                inputs,
                output_rates,
            });
        }

        assert_eq!(c.i16(), 0, "expected zero variants");
        assert_eq!(c.pos, bytes.len(), "trailing bytes after synthdef");

        // Validate node references and topological order.
        for (i, u) in ugens.iter().enumerate() {
            for input in &u.inputs {
                if let DecodedInput::Node(src, out) = input {
                    assert!(*src < i, "UGen {} references non-preceding UGen {}", i, src);
                    assert!(
                        *out < ugens[*src].output_rates.len(),
                        "UGen {} references invalid output {} of UGen {}",
                        i,
                        out,
                        src
                    );
                }
            }
        }

        DecodedSynthDef {
            name,
            constants,
            param_defaults,
            param_names,
            ugens,
        }
    }

    /// Signature of a UGen with node references remapped through `placed`
    /// (old index → canonical index). Only valid once all node inputs of the
    /// UGen are placed.
    fn ugen_signature(u: &DecodedUGen, placed: &[Option<usize>]) -> String {
        let inputs: Vec<String> = u
            .inputs
            .iter()
            .map(|input| match input {
                DecodedInput::Const(v) => format!("c{:08x}", v.to_bits()),
                DecodedInput::Node(src, out) => format!(
                    "n{}:{}",
                    placed[*src].expect("signature of node with unplaced input"),
                    out
                ),
            })
            .collect();
        format!(
            "{}|rate{}|special{}|in[{}]|outr{:?}",
            u.class,
            u.rate,
            u.special,
            inputs.join(","),
            u.output_rates
        )
    }

    /// Canonicalize the UGen list: remap UGen indices by a deterministic
    /// topological order (ready node with the lexicographically smallest
    /// signature first; ties broken by original index) and serialize each
    /// UGen with remapped input edges. Two graphs are structurally equivalent
    /// iff their canonical serializations are equal — insensitive to UGen
    /// reordering and constant-table permutation, sensitive to any change in
    /// class, rate, special index, wiring or output rates.
    fn canonical_ugens(def: &DecodedSynthDef) -> Vec<String> {
        let n = def.ugens.len();
        let mut placed: Vec<Option<usize>> = vec![None; n];
        let mut result = Vec::with_capacity(n);

        for next_idx in 0..n {
            let mut best: Option<(String, usize)> = None;
            for (i, u) in def.ugens.iter().enumerate() {
                if placed[i].is_some() {
                    continue;
                }
                let ready = u.inputs.iter().all(|input| match input {
                    DecodedInput::Const(_) => true,
                    DecodedInput::Node(src, _) => placed[*src].is_some(),
                });
                if !ready {
                    continue;
                }
                let sig = ugen_signature(u, &placed);
                match &best {
                    None => best = Some((sig, i)),
                    Some((best_sig, _)) if sig < *best_sig => best = Some((sig, i)),
                    _ => {}
                }
            }
            let (sig, i) = best.expect("no ready UGen — cycle in graph?");
            placed[i] = Some(next_idx);
            result.push(sig);
        }

        result
    }

    /// Assert structural equivalence of two single-synthdef SCgf v2 blobs:
    /// same name, same parameter names/slots/defaults, same constant set
    /// (with tolerance), and canonically identical UGen graphs.
    fn assert_equivalent(reference: &[u8], candidate: &[u8]) {
        let r = decode_synthdef(reference);
        let c = decode_synthdef(candidate);

        assert_eq!(r.name, c.name, "synthdef name mismatch");
        assert_eq!(
            r.param_defaults, c.param_defaults,
            "{}: param default slots mismatch",
            r.name
        );
        assert_eq!(
            r.param_names, c.param_names,
            "{}: param names/indices mismatch",
            r.name
        );

        let mut rc = r.constants.clone();
        let mut cc = c.constants.clone();
        rc.sort_by(f32::total_cmp);
        cc.sort_by(f32::total_cmp);
        assert_eq!(
            rc.len(),
            cc.len(),
            "{}: constant count mismatch ({:?} vs {:?})",
            r.name,
            rc,
            cc
        );
        for (a, b) in rc.iter().zip(cc.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "{}: constant set mismatch ({:?} vs {:?})",
                r.name,
                rc,
                cc
            );
        }

        let r_canon = canonical_ugens(&r);
        let c_canon = canonical_ugens(&c);
        assert_eq!(
            r_canon, c_canon,
            "{}: UGen graphs are not structurally equivalent",
            r.name
        );
    }

    // ---------------------------------------------------------------------
    // Equivalence tests: builder-based (production) vs legacy hand-encoded
    // ---------------------------------------------------------------------

    // ---------------------------------------------------------------------
    // Structural de-click tests for system_link_audio[_mono]
    //
    // These replaced the equiv_* tests when the link synthdefs gained Lag
    // smoothing and the mute gate (the legacy encoders were frozen
    // pre-de-click graphs, so byte-level equivalence no longer applies).
    // ---------------------------------------------------------------------

    /// Assert the de-click topology of `system_link_audio[_mono]`:
    ///
    /// - param surface `inbus, outbus, amp, pan, mute` with `mute` default 0
    /// - three kr `Lag(_, DECLICK_LAG_S)` smoothers on amp/pan/mute
    /// - amp path: `In[ch] × Lag(amp)` (audio) on both channels
    /// - pan law reads `Lag(pan)`
    /// - mute gate: `1 − Lag(mute)` (kr) multiplied into both channels (audio)
    /// - Out and metering are fed from the gated signal
    fn assert_link_audio_declick_structure(bytes: &[u8], mono: bool) {
        const KR: u8 = 1;
        const AR: u8 = 2;

        let def = decode_synthdef(bytes);

        // Parameter surface: mute appended at slot 4, default 0 (unmuted).
        let names: Vec<(&str, usize)> = def
            .param_names
            .iter()
            .map(|(n, i)| (n.as_str(), *i))
            .collect();
        assert_eq!(
            names,
            vec![
                ("inbus", 0),
                ("outbus", 1),
                ("amp", 2),
                ("pan", 3),
                ("mute", 4)
            ],
            "{}: param names/slots",
            def.name
        );
        assert_eq!(
            def.param_defaults,
            vec![0.0, 0.0, 1.0, 0.0, 0.0],
            "{}: param defaults (mute must default to 0)",
            def.name
        );

        // Exactly one audio-rate In reader.
        let in_idx = def
            .ugens
            .iter()
            .position(|u| u.class == "In" && u.rate == AR)
            .expect("In.ar reader missing");

        // Three kr Lag smoothers, one per mixer control (amp=2, pan=3,
        // mute=4), each with lag time DECLICK_LAG_S.
        let mut lag_by_slot = std::collections::HashMap::new();
        for (i, u) in def.ugens.iter().enumerate() {
            if u.class != "Lag" {
                continue;
            }
            assert_eq!(u.rate, KR, "{}: Lag must be control-rate", def.name);
            assert_eq!(u.inputs.len(), 2, "{}: Lag input count", def.name);
            let slot = match u.inputs[0] {
                DecodedInput::Node(0, slot) => slot,
                ref other => panic!("{}: Lag input 0 not a Control param: {:?}", def.name, other),
            };
            match u.inputs[1] {
                DecodedInput::Const(v) => assert!(
                    (v - DECLICK_LAG_S).abs() < 1e-6,
                    "{}: Lag time {} != DECLICK_LAG_S",
                    def.name,
                    v
                ),
                ref other => panic!("{}: Lag time not a constant: {:?}", def.name, other),
            }
            assert!(
                lag_by_slot.insert(slot, i).is_none(),
                "{}: duplicate Lag on param slot {}",
                def.name,
                slot
            );
        }
        let mut slots: Vec<usize> = lag_by_slot.keys().copied().collect();
        slots.sort_unstable();
        assert_eq!(
            slots,
            vec![2, 3, 4],
            "{}: expected Lag on amp (2), pan (3), mute (4)",
            def.name
        );
        let lag_amp = lag_by_slot[&2];
        let lag_pan = lag_by_slot[&3];
        let lag_mute = lag_by_slot[&4];

        // Helper: audio-rate multiplies reading a given kr node.
        let audio_muls_by = |src: usize| -> Vec<usize> {
            def.ugens
                .iter()
                .enumerate()
                .filter(|(_, u)| {
                    u.class == "BinaryOpUGen"
                        && u.rate == AR
                        && u.special == OP_MUL
                        && u.inputs.contains(&DecodedInput::Node(src, 0))
                })
                .map(|(i, _)| i)
                .collect()
        };

        // amp → Lag → multiply: both In channels are scaled by the lagged amp.
        let amp_muls = audio_muls_by(lag_amp);
        assert_eq!(
            amp_muls.len(),
            2,
            "{}: expected 2 audio multiplies by Lag(amp)",
            def.name
        );
        for (mul_idx, ch) in amp_muls.iter().zip([0usize, 1usize]) {
            assert!(
                def.ugens[*mul_idx]
                    .inputs
                    .contains(&DecodedInput::Node(in_idx, ch)),
                "{}: amp multiply {} must read In channel {}",
                def.name,
                mul_idx,
                ch
            );
        }

        // The pan law (kr max/min against 0.0) must read the lagged pan.
        for op in [OP_MAX, OP_MIN] {
            assert!(
                def.ugens.iter().any(|u| {
                    u.class == "BinaryOpUGen"
                        && u.rate == KR
                        && u.special == op
                        && u.inputs.contains(&DecodedInput::Node(lag_pan, 0))
                }),
                "{}: pan law (special {}) must read Lag(pan)",
                def.name,
                op
            );
        }

        // Mute gate: kr `1.0 - Lag(mute)`, multiplied into both channels.
        let gate_idx = def
            .ugens
            .iter()
            .position(|u| {
                u.class == "BinaryOpUGen"
                    && u.rate == KR
                    && u.special == OP_SUB
                    && u.inputs
                        == vec![DecodedInput::Const(1.0), DecodedInput::Node(lag_mute, 0)]
            })
            .expect("mute gate (1 - Lag(mute)) missing");
        let gated = audio_muls_by(gate_idx);
        assert_eq!(
            gated.len(),
            2,
            "{}: expected 2 audio multiplies by the mute gate",
            def.name
        );

        // Out is fed from the gated signal (mono: via the L+R sum).
        let outs: Vec<&DecodedUGen> = def.ugens.iter().filter(|u| u.class == "Out").collect();
        assert_eq!(outs.len(), 1, "{}: exactly one Out", def.name);
        let out = outs[0];
        if mono {
            assert_eq!(out.inputs.len(), 2, "{}: mono Out = bus + 1 ch", def.name);
            let sum_idx = match out.inputs[1] {
                DecodedInput::Node(i, 0) => i,
                ref other => panic!("{}: mono Out signal input: {:?}", def.name, other),
            };
            let sum = &def.ugens[sum_idx];
            assert!(
                sum.class == "BinaryOpUGen" && sum.special == OP_ADD,
                "{}: mono Out must be fed by the L+R sum",
                def.name
            );
            for g in &gated {
                assert!(
                    sum.inputs.contains(&DecodedInput::Node(*g, 0)),
                    "{}: mono sum must read gated channel {}",
                    def.name,
                    g
                );
            }
        } else {
            assert_eq!(out.inputs.len(), 3, "{}: stereo Out = bus + 2 ch", def.name);
            for g in &gated {
                assert!(
                    out.inputs.contains(&DecodedInput::Node(*g, 0)),
                    "{}: stereo Out must read gated channel {}",
                    def.name,
                    g
                );
            }
        }

        // Metering survives and taps the post-gate signal.
        let count = |class: &str| def.ugens.iter().filter(|u| u.class == class).count();
        assert_eq!(count("Impulse"), 1, "{}: Impulse", def.name);
        assert_eq!(count("Peak"), 2, "{}: Peak", def.name);
        assert_eq!(count("Amplitude"), 2, "{}: Amplitude", def.name);
        assert_eq!(count("SendTrig"), 4, "{}: SendTrig", def.name);
        for u in def
            .ugens
            .iter()
            .filter(|u| u.class == "Peak" || u.class == "Amplitude")
        {
            match u.inputs[0] {
                DecodedInput::Node(src, 0) => assert!(
                    gated.contains(&src),
                    "{}: {} must tap the gated signal",
                    def.name,
                    u.class
                ),
                ref other => panic!("{}: meter input: {:?}", def.name, other),
            }
        }
    }

    #[test]
    fn declick_system_link_audio() {
        assert_link_audio_declick_structure(&create_system_link_audio_bytes().unwrap(), false);
    }

    #[test]
    fn declick_system_link_audio_mono() {
        assert_link_audio_declick_structure(&create_system_link_audio_mono_bytes().unwrap(), true);
    }

    #[test]
    fn equiv_port_to_group_link_1() {
        assert_equivalent(
            &legacy::create_port_to_group_link_1_bytes().unwrap(),
            &create_port_to_group_link_1_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_port_to_group_link_2() {
        assert_equivalent(
            &legacy::create_port_to_group_link_2_bytes().unwrap(),
            &create_port_to_group_link_2_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_input_link_1() {
        assert_equivalent(
            &legacy::create_input_link_1_bytes().unwrap(),
            &create_input_link_1_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_input_link_2() {
        assert_equivalent(
            &legacy::create_input_link_2_bytes().unwrap(),
            &create_input_link_2_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_port_tr_to_param_link_1() {
        assert_equivalent(
            &legacy::create_port_tr_to_param_link_1_bytes().unwrap(),
            &create_port_tr_to_param_link_1_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_a2k_adapter_1() {
        assert_equivalent(
            &legacy::create_a2k_adapter_1_bytes().unwrap(),
            &create_a2k_adapter_1_bytes().unwrap(),
        );
    }

    #[test]
    fn equiv_param_kr_modulate_all_arities() {
        for n in 1..=PARAM_KR_MODULATE_MAX {
            assert_equivalent(
                &legacy::create_param_kr_modulate_n_bytes(n).unwrap(),
                &create_param_kr_modulate_n_bytes(n).unwrap(),
            );
        }
    }

    /// Guard against a vacuously-passing harness: two genuinely different
    /// graphs (ar passthrough vs kr passthrough) must canonicalize to
    /// different serializations.
    #[test]
    fn harness_detects_structural_difference() {
        let a = decode_synthdef(&create_input_link_1_bytes().unwrap());
        let b = decode_synthdef(&create_port_tr_to_param_link_1_bytes().unwrap());
        assert_ne!(canonical_ugens(&a), canonical_ugens(&b));

        let mono = decode_synthdef(&create_system_link_audio_mono_bytes().unwrap());
        let stereo = decode_synthdef(&create_system_link_audio_bytes().unwrap());
        assert_ne!(canonical_ugens(&mono), canonical_ugens(&stereo));
    }

    // ---------------------------------------------------------------------
    // Original sanity tests (now exercising the builder-based encoders)
    // ---------------------------------------------------------------------

    #[test]
    fn test_create_system_link_audio_bytes() {
        let bytes = create_system_link_audio_bytes().unwrap();
        // Check magic header
        assert_eq!(&bytes[0..4], b"SCgf");
        // Should have reasonable size
        assert!(bytes.len() > 100);
    }

    #[test]
    fn test_create_system_link_audio_mono_bytes() {
        let bytes = create_system_link_audio_mono_bytes().unwrap();

        // Magic + version 2.
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());

        // Synthdef name "system_link_audio_mono" must be encoded.
        let needle = b"system_link_audio_mono";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes",
        );

        // Same parameter surface as the stereo synthdef.
        assert!(bytes.windows(5).any(|w| w == b"inbus"));
        assert!(bytes.windows(6).any(|w| w == b"outbus"));
        assert!(bytes.windows(3).any(|w| w == b"amp"));
        assert!(bytes.windows(3).any(|w| w == b"pan"));
        assert!(bytes.windows(4).any(|w| w == b"mute"));

        // The mono synthdef must end with an `Out` UGen that has exactly one
        // signal input (bus + 1 channel = 2 inputs total). The stereo synthdef
        // writes 3 inputs (bus + 2 channels), so this assertion catches
        // accidental regressions back to a stereo Out.ar.
        //
        // 27 UGens: the pre-de-click 21 plus 3 Lag smoothers, 1 mute-gate
        // subtract and 2 gate multiplies.
        let def = decode_synthdef(&bytes);
        assert_eq!(def.ugens.len(), 27, "expected 27 UGens in mono variant");

        let outs: Vec<&DecodedUGen> = def.ugens.iter().filter(|u| u.class == "Out").collect();
        assert_eq!(outs.len(), 1, "should be exactly one Out UGen");
        assert_eq!(
            outs[0].inputs.len(),
            2,
            "Out.ar must have 2 inputs (bus + 1 channel) for mono mixdown",
        );
    }

    #[test]
    fn test_create_port_to_group_link_1_bytes() {
        let bytes = create_port_to_group_link_1_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());
        // Name "port_to_group_link_1" must be present in the encoded body.
        let needle = b"port_to_group_link_1";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        // in_bus / out_bus param names must be encoded.
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
    }

    #[test]
    fn test_create_port_to_group_link_2_bytes() {
        let bytes = create_port_to_group_link_2_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        let needle = b"port_to_group_link_2";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
    }

    #[test]
    fn test_create_input_link_1_bytes() {
        let bytes = create_input_link_1_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());
        let needle = b"input_link_1";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
    }

    #[test]
    fn test_create_input_link_2_bytes() {
        let bytes = create_input_link_2_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        let needle = b"input_link_2";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
    }

    #[test]
    fn test_create_port_tr_to_param_link_1_bytes() {
        let bytes = create_port_tr_to_param_link_1_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());
        let needle = b"port_tr_to_param_link_1";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
    }

    #[test]
    fn test_create_a2k_adapter_1_bytes() {
        let bytes = create_a2k_adapter_1_bytes().unwrap();
        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());
        let needle = b"a2k_adapter_1";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "synthdef name not found in encoded bytes"
        );
        assert!(bytes.windows(6).any(|w| w == b"in_bus"));
        assert!(bytes.windows(7).any(|w| w == b"out_bus"));
        // The A2K UGen name must appear so we know the synthdef invokes the
        // rate-conversion UGen rather than passing audio straight through.
        assert!(bytes.windows(3).any(|w| w == b"A2K"));
    }

    #[test]
    fn test_create_param_kr_modulate_n_bytes_each_arity() {
        for n in 1..=PARAM_KR_MODULATE_MAX {
            let bytes = create_param_kr_modulate_n_bytes(n).unwrap();
            assert_eq!(&bytes[0..4], b"SCgf");
            let want_name = format!("param_kr_modulate_{}", n);
            assert!(
                bytes
                    .windows(want_name.len())
                    .any(|w| w == want_name.as_bytes()),
                "name {} not found in encoded bytes",
                want_name,
            );
            // Each arity must declare baseline, `in_<i>` / `scale_<i>` /
            // `offset_<i>` for every i in 0..n, and out_bus.
            assert!(bytes.windows(8).any(|w| w == b"baseline"));
            let letters = [b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'];
            for i in 0..n {
                let in_name = [b'i', b'n', b'_', letters[i]];
                assert!(
                    bytes.windows(in_name.len()).any(|w| w == in_name),
                    "param in_{} not found in {}-arity",
                    letters[i] as char,
                    n,
                );
                let scale_name = [b's', b'c', b'a', b'l', b'e', b'_', letters[i]];
                assert!(
                    bytes.windows(scale_name.len()).any(|w| w == scale_name),
                    "param scale_{} not found in {}-arity",
                    letters[i] as char,
                    n,
                );
                let offset_name = [b'o', b'f', b'f', b's', b'e', b't', b'_', letters[i]];
                assert!(
                    bytes.windows(offset_name.len()).any(|w| w == offset_name),
                    "param offset_{} not found in {}-arity",
                    letters[i] as char,
                    n,
                );
            }
            assert!(bytes.windows(7).any(|w| w == b"out_bus"));
        }
    }

    #[test]
    fn test_create_param_kr_modulate_n_bytes_default_values() {
        // Per-source defaults: scale=1.0, offset=0.0. baseline default 0.0.
        for n in 1..=PARAM_KR_MODULATE_MAX {
            let bytes = create_param_kr_modulate_n_bytes(n).unwrap();
            let def = decode_synthdef(&bytes);
            let defaults = &def.param_defaults;
            assert_eq!(defaults.len(), 3 * n + 2, "param slot count (n={})", n);
            assert_eq!(defaults[0], 0.0, "baseline default 0.0 (n={})", n);
            for i in 0..n {
                assert_eq!(defaults[1 + 3 * i], 0.0, "in_<i> default 0.0");
                assert_eq!(defaults[2 + 3 * i], 1.0, "scale_<i> default 1.0");
                assert_eq!(defaults[3 + 3 * i], 0.0, "offset_<i> default 0.0");
            }
            assert_eq!(defaults[1 + 3 * n], 0.0, "out_bus default 0.0");
        }
    }

    #[test]
    #[should_panic]
    fn test_create_param_kr_modulate_n_bytes_rejects_zero() {
        let _ = create_param_kr_modulate_n_bytes(0);
    }

    #[test]
    #[should_panic]
    fn test_create_param_kr_modulate_n_bytes_rejects_above_max() {
        let _ = create_param_kr_modulate_n_bytes(PARAM_KR_MODULATE_MAX + 1);
    }
}
