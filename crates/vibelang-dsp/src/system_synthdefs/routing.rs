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
}
