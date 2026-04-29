//! System SynthDef for audio bus routing.
//!
//! This module builds the `system_link_audio` synthdef used for group bus routing.
//! The synthdef is built using direct binary encoding for efficiency.
//!
//! ## Signal Flow
//!
//! ```text
//! In.ar(inbus) → × amp → balance(pan) → Out.ar(outbus)
//!                                ↓
//!                    Peak + Amplitude → SendTrig (at 20Hz)
//! ```
//!
//! ## SendTrig IDs
//!
//! - 0: peak_left
//! - 1: peak_right
//! - 2: rms_left
//! - 3: rms_right

use std::io::Write;

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
///
/// # Metering
///
/// The synthdef sends meter data via SendTrig at 20Hz:
/// - Trigger ID 0: Left channel peak
/// - Trigger ID 1: Right channel peak
/// - Trigger ID 2: Left channel RMS (via Amplitude UGen)
/// - Trigger ID 3: Right channel RMS (via Amplitude UGen)
pub fn create_system_link_audio_bytes() -> Result<Vec<u8>, std::io::Error> {
    let mut buf = Vec::new();

    // File header
    buf.write_all(b"SCgf")?; // Magic
    buf.write_all(&2i32.to_be_bytes())?; // Version 2
    buf.write_all(&1i16.to_be_bytes())?; // Number of synthdefs

    // SynthDef name
    let name = b"system_link_audio";
    buf.push(name.len() as u8);
    buf.write_all(name)?;

    // Constants (7 total)
    // 0: 0.0 (SendTrig ID 0 for peak_left, also zero for max/min)
    // 1: 1.0 (SendTrig ID 1 for peak_right, also 1.0 for gain calc)
    // 2: 2.0 (SendTrig ID 2 for rms_left)
    // 3: 3.0 (SendTrig ID 3 for rms_right)
    // 4: 20.0 (Impulse frequency - 20Hz for meter updates)
    // 5: 0.01 (Amplitude attack time)
    // 6: 0.1 (Amplitude release time)
    buf.write_all(&7i32.to_be_bytes())?; // num constants
    buf.write_all(&0.0f32.to_be_bytes())?; // constant 0
    buf.write_all(&1.0f32.to_be_bytes())?; // constant 1
    buf.write_all(&2.0f32.to_be_bytes())?; // constant 2
    buf.write_all(&3.0f32.to_be_bytes())?; // constant 3
    buf.write_all(&20.0f32.to_be_bytes())?; // constant 4
    buf.write_all(&0.01f32.to_be_bytes())?; // constant 5
    buf.write_all(&0.1f32.to_be_bytes())?; // constant 6

    // Parameters: inbus=0, outbus=0, amp=1.0, pan=0.0
    buf.write_all(&4i32.to_be_bytes())?; // num params
    buf.write_all(&0.0f32.to_be_bytes())?; // inbus default = 0
    buf.write_all(&0.0f32.to_be_bytes())?; // outbus default = 0
    buf.write_all(&1.0f32.to_be_bytes())?; // amp default = 1.0
    buf.write_all(&0.0f32.to_be_bytes())?; // pan default = 0.0

    // Param names
    buf.write_all(&4i32.to_be_bytes())?; // num param names
    let inbus_name = b"inbus";
    buf.push(inbus_name.len() as u8);
    buf.write_all(inbus_name)?;
    buf.write_all(&0i32.to_be_bytes())?; // index 0
    let outbus_name = b"outbus";
    buf.push(outbus_name.len() as u8);
    buf.write_all(outbus_name)?;
    buf.write_all(&1i32.to_be_bytes())?; // index 1
    let amp_name = b"amp";
    buf.push(amp_name.len() as u8);
    buf.write_all(amp_name)?;
    buf.write_all(&2i32.to_be_bytes())?; // index 2
    let pan_name = b"pan";
    buf.push(pan_name.len() as u8);
    buf.write_all(pan_name)?;
    buf.write_all(&3i32.to_be_bytes())?; // index 3

    // UGens (20 total)
    //
    // 0:  Control (4 outputs: inbus, outbus, amp, pan)
    // 1:  In.ar (2 outputs: left, right)
    // 2:  BinaryOp * (left * amp)          = scaled_left
    // 3:  BinaryOp * (right * amp)         = scaled_right
    // 4:  BinaryOp max(0, pan)             = max_pan       [kr]
    // 5:  BinaryOp 1 - max_pan             = left_gain     [kr]
    // 6:  BinaryOp min(0, pan)             = min_pan       [kr]
    // 7:  BinaryOp 1 + min_pan             = right_gain    [kr]
    // 8:  BinaryOp scaled_left * left_gain = panned_left   [ar]
    // 9:  BinaryOp scaled_right * right_gain = panned_right [ar]
    // 10: Out.ar (outbus, panned_left, panned_right)
    // 11: Impulse.kr (20Hz)
    // 12-13: Peak.kr (panned_left, panned_right)
    // 14-15: Amplitude.kr (panned_left, panned_right)
    // 16-19: SendTrig.kr (peak_l, peak_r, rms_l, rms_r)
    buf.write_all(&20i32.to_be_bytes())?; // num ugens

    let binop_name = b"BinaryOpUGen";

    // UGen 0: Control (control rate, 4 outputs: inbus, outbus, amp, pan)
    let control_name = b"Control";
    buf.push(control_name.len() as u8);
    buf.write_all(control_name)?;
    buf.push(1); // rate: control
    buf.write_all(&0i32.to_be_bytes())?; // num inputs
    buf.write_all(&4i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    buf.push(1); // output 0 rate: control (inbus)
    buf.push(1); // output 1 rate: control (outbus)
    buf.push(1); // output 2 rate: control (amp)
    buf.push(1); // output 3 rate: control (pan)

    // UGen 1: In.ar (audio rate, 2 outputs: left, right)
    let in_name = b"In";
    buf.push(in_name.len() as u8);
    buf.write_all(in_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&1i32.to_be_bytes())?; // num inputs
    buf.write_all(&2i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 0)?; // input: Control[0] (inbus)
    buf.push(2); // output 0 rate: audio (left)
    buf.push(2); // output 1 rate: audio (right)

    // UGen 2: left * amp - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special: multiply
    write_ugen_input(&mut buf, 1, 0)?; // In[0] (left)
    write_ugen_input(&mut buf, 0, 2)?; // Control[2] (amp)
    buf.push(2); // output rate: audio

    // UGen 3: right * amp - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special: multiply
    write_ugen_input(&mut buf, 1, 1)?; // In[1] (right)
    write_ugen_input(&mut buf, 0, 2)?; // Control[2] (amp)
    buf.push(2); // output rate: audio

    // UGen 4: max(0, pan) - control rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&13i16.to_be_bytes())?; // special: max
    write_const_input(&mut buf, 0)?; // constant 0 (0.0)
    write_ugen_input(&mut buf, 0, 3)?; // Control[3] (pan)
    buf.push(1); // output rate: control

    // UGen 5: 1 - max(0, pan) = left_gain - control rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&1i16.to_be_bytes())?; // special: subtract
    write_const_input(&mut buf, 1)?; // constant 1 (1.0)
    write_ugen_input(&mut buf, 4, 0)?; // UGen4[0] (max_pan)
    buf.push(1); // output rate: control

    // UGen 6: min(0, pan) - control rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&12i16.to_be_bytes())?; // special: min
    write_const_input(&mut buf, 0)?; // constant 0 (0.0)
    write_ugen_input(&mut buf, 0, 3)?; // Control[3] (pan)
    buf.push(1); // output rate: control

    // UGen 7: 1 + min(0, pan) = right_gain - control rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special: add
    write_const_input(&mut buf, 1)?; // constant 1 (1.0)
    write_ugen_input(&mut buf, 6, 0)?; // UGen6[0] (min_pan)
    buf.push(1); // output rate: control

    // UGen 8: scaled_left * left_gain = panned_left - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special: multiply
    write_ugen_input(&mut buf, 2, 0)?; // UGen2[0] (scaled_left)
    write_ugen_input(&mut buf, 5, 0)?; // UGen5[0] (left_gain)
    buf.push(2); // output rate: audio

    // UGen 9: scaled_right * right_gain = panned_right - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special: multiply
    write_ugen_input(&mut buf, 3, 0)?; // UGen3[0] (scaled_right)
    write_ugen_input(&mut buf, 7, 0)?; // UGen7[0] (right_gain)
    buf.push(2); // output rate: audio

    // UGen 10: Out.ar (outputs panned audio to outbus)
    let out_name = b"Out";
    buf.push(out_name.len() as u8);
    buf.write_all(out_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 1)?; // Control[1] (outbus)
    write_ugen_input(&mut buf, 8, 0)?; // UGen8[0] (panned_left)
    write_ugen_input(&mut buf, 9, 0)?; // UGen9[0] (panned_right)

    // UGen 11: Impulse.kr (20Hz trigger for meter updates)
    let impulse_name = b"Impulse";
    buf.push(impulse_name.len() as u8);
    buf.write_all(impulse_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs (freq, phase)
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_const_input(&mut buf, 4)?; // constant 4 (20.0 Hz)
    write_const_input(&mut buf, 0)?; // constant 0 (0.0 phase)
    buf.push(1); // output rate: control

    // UGen 12: Peak.kr (left channel peak, reset by Impulse)
    let peak_name = b"Peak";
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 8, 0)?; // UGen8[0] (panned_left)
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    buf.push(1); // output rate: control

    // UGen 13: Peak.kr (right channel peak, reset by Impulse)
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 9, 0)?; // UGen9[0] (panned_right)
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    buf.push(1); // output rate: control

    // UGen 14: Amplitude.kr (left channel RMS-like)
    let amplitude_name = b"Amplitude";
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 8, 0)?; // UGen8[0] (panned_left)
    write_const_input(&mut buf, 5)?; // constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 15: Amplitude.kr (right channel RMS-like)
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 9, 0)?; // UGen9[0] (panned_right)
    write_const_input(&mut buf, 5)?; // constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 16: SendTrig.kr (send peak_left)
    let sendtrig_name = b"SendTrig";
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    write_const_input(&mut buf, 0)?; // constant 0 (ID = 0)
    write_ugen_input(&mut buf, 12, 0)?; // UGen12[0] (Peak left)

    // UGen 17: SendTrig.kr (send peak_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    write_const_input(&mut buf, 1)?; // constant 1 (ID = 1)
    write_ugen_input(&mut buf, 13, 0)?; // UGen13[0] (Peak right)

    // UGen 18: SendTrig.kr (send rms_left)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    write_const_input(&mut buf, 2)?; // constant 2 (ID = 2)
    write_ugen_input(&mut buf, 14, 0)?; // UGen14[0] (Amplitude left)

    // UGen 19: SendTrig.kr (send rms_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 11, 0)?; // UGen11[0] (Impulse)
    write_const_input(&mut buf, 3)?; // constant 3 (ID = 3)
    write_ugen_input(&mut buf, 15, 0)?; // UGen15[0] (Amplitude right)

    // Variants: none
    buf.write_all(&0i16.to_be_bytes())?;

    Ok(buf)
}

/// Write a constant input reference to the buffer.
fn write_const_input(buf: &mut Vec<u8>, const_idx: i32) -> std::io::Result<()> {
    buf.write_all(&(-1i32).to_be_bytes())?; // -1 means constant
    buf.write_all(&const_idx.to_be_bytes())?;
    Ok(())
}

/// Write a UGen input reference to the buffer.
fn write_ugen_input(buf: &mut Vec<u8>, ugen_idx: i32, output_idx: i32) -> std::io::Result<()> {
    buf.write_all(&ugen_idx.to_be_bytes())?;
    buf.write_all(&output_idx.to_be_bytes())?;
    Ok(())
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

/// Maximum number of source kr signals supported by `param_kr_modulate_<n>`.
///
/// Sets the upper bound that [`create_param_kr_modulate_n_bytes`] will accept
/// and caps how many `modulate_by` sources can target one `(voice, param)`
/// pair without truncation. Eight is a comfortable headroom for typical
/// modular patches (env + lfo + macro + offset + …) without bloating the
/// synthdef set.
pub const PARAM_KR_MODULATE_MAX: usize = 8;

/// Create the `param_kr_modulate_<n>` synthdef bytes for the given source count.
///
/// The synthdef adds a `baseline` control to `n` summed control-rate input
/// buses and writes the result onto a single intermediate control bus. Used
/// by `RoutesHandler::finalize_params` for the BEND path of multi-output v2
/// (the user's `set_param` value lives in `baseline`; modulators bend around
/// it). Equivalent to:
///
/// ```supercollider
/// SynthDef("param_kr_modulate_<n>", { |baseline=0, in_a=0, in_b=0, ..., out_bus=0|
///     Out.kr(out_bus, baseline + In.kr(in_a, 1) + In.kr(in_b, 1) + ...)
/// })
/// ```
///
/// `n` must be in `1..=PARAM_KR_MODULATE_MAX`. At `n=1` the synthdef is still
/// a summer (`baseline + In.kr(in_a, 1)`) — that's the whole point of BEND
/// vs SET: the summer is *always* present so the user's `set_param` value
/// can ride underneath the modulator. The `n` source bus parameters are
/// named `in_a`, `in_b`, … in declaration order; the destination bus
/// parameter is `out_bus`. Allocation and teardown of the intermediate bus
/// + summer node is owned by
/// [`crates/vibelang-core::handlers::routes::RoutesHandler::finalize_params`].
pub fn create_param_kr_modulate_n_bytes(n: usize) -> Result<Vec<u8>, std::io::Error> {
    assert!(
        (1..=PARAM_KR_MODULATE_MAX).contains(&n),
        "param_kr_modulate_<n> only supports n in 1..={} (got {})",
        PARAM_KR_MODULATE_MAX,
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

    // Parameters: baseline, in_a, in_b, ..., out_bus (all defaulting to 0).
    // Total: 1 (baseline) + n (sources) + 1 (out_bus) = n + 2.
    let num_params = (n + 2) as i32;
    buf.write_all(&num_params.to_be_bytes())?;
    for _ in 0..num_params {
        buf.write_all(&0.0f32.to_be_bytes())?;
    }

    // Param names: baseline (idx 0), in_a..in_<n-th> (idx 1..=n), out_bus (idx n+1).
    let port_letters = [b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'];
    buf.write_all(&num_params.to_be_bytes())?;
    let baseline_name = b"baseline";
    buf.push(baseline_name.len() as u8);
    buf.write_all(baseline_name)?;
    buf.write_all(&0i32.to_be_bytes())?;
    for i in 0..n {
        let pname = [b'i', b'n', b'_', port_letters[i]];
        buf.push(pname.len() as u8);
        buf.write_all(&pname)?;
        buf.write_all(&((i + 1) as i32).to_be_bytes())?;
    }
    let out_bus_name = b"out_bus";
    buf.push(out_bus_name.len() as u8);
    buf.write_all(out_bus_name)?;
    buf.write_all(&((n + 1) as i32).to_be_bytes())?;

    // UGen layout (all kr-rate):
    //   0           : Control with (n + 2) outputs (baseline, in_a..n, out_bus)
    //   1 ..= n     : In.kr(in_<letter>, 1) — one per source
    //   n+1 ..= 2n  : BinaryOpUGen add — n cumulative sums
    //                    (baseline + in_a, then + in_b, …)
    //   2n+1        : Out.kr(out_bus, final_sum)
    let num_ugens = (2 * n + 2) as i32;
    buf.write_all(&num_ugens.to_be_bytes())?;

    // UGen 0: Control (kr) with (n + 2) outputs
    let control_name = b"Control";
    buf.push(control_name.len() as u8);
    buf.write_all(control_name)?;
    buf.push(1); // control rate
    buf.write_all(&0i32.to_be_bytes())?; // 0 inputs
    buf.write_all(&((n + 2) as i32).to_be_bytes())?; // n+2 outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    for _ in 0..(n + 2) {
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
        // Control[1+i] = in_<letter>; Control[0] is baseline.
        write_ugen_input(&mut buf, 0, (i + 1) as i32)?;
        buf.push(1); // output rate
    }

    // UGens n+1..=2n: cumulative BinaryOpUGen add (special index 0).
    //   sum_0 = baseline + In_0  (i.e. Control[0] + UGen[1])
    //   sum_k = sum_{k-1} + In_k (k = 1..n-1)
    let binop_name = b"BinaryOpUGen";
    for k in 0..n {
        buf.push(binop_name.len() as u8);
        buf.write_all(binop_name)?;
        buf.push(1); // control rate
        buf.write_all(&2i32.to_be_bytes())?; // 2 inputs
        buf.write_all(&1i32.to_be_bytes())?; // 1 output
        buf.write_all(&0i16.to_be_bytes())?; // special index 0 = add
        if k == 0 {
            // First sum: baseline + In_0
            write_ugen_input(&mut buf, 0, 0)?; // Control[0] = baseline
            write_ugen_input(&mut buf, 1, 0)?; // UGen 1 = In.kr(in_a)
        } else {
            // Subsequent sum: previous BinaryOp + next In
            // previous BinaryOp lives at UGen index n + k
            // next In lives at UGen index k + 1 (In_k)
            write_ugen_input(&mut buf, (n as i32) + (k as i32), 0)?;
            write_ugen_input(&mut buf, (k as i32) + 1, 0)?;
        }
        buf.push(1); // output rate
    }

    // UGen 2n+1: Out.kr(out_bus, final_sum)
    let out_name = b"Out";
    buf.push(out_name.len() as u8);
    buf.write_all(out_name)?;
    buf.push(1); // control rate
    buf.write_all(&2i32.to_be_bytes())?; // 2 inputs (bus index + signal)
    buf.write_all(&0i32.to_be_bytes())?; // 0 outputs
    buf.write_all(&0i16.to_be_bytes())?;
    write_ugen_input(&mut buf, 0, (n + 1) as i32)?; // Control[n+1] = out_bus
    write_ugen_input(&mut buf, (2 * n) as i32, 0)?; // last BinaryOp output

    // No variants
    buf.write_all(&0i16.to_be_bytes())?;

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_system_link_audio_bytes() {
        let bytes = create_system_link_audio_bytes().unwrap();
        // Check magic header
        assert_eq!(&bytes[0..4], b"SCgf");
        // Should have reasonable size
        assert!(bytes.len() > 100);
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
            // Each arity must declare baseline, `in_a`..`in_<n-th letter>`, out_bus.
            assert!(bytes.windows(8).any(|w| w == b"baseline"));
            let letters = [b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h'];
            for i in 0..n {
                let pname = [b'i', b'n', b'_', letters[i]];
                assert!(
                    bytes.windows(pname.len()).any(|w| w == pname),
                    "param in_{} not found in {}-arity",
                    letters[i] as char,
                    n,
                );
            }
            assert!(bytes.windows(7).any(|w| w == b"out_bus"));
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
