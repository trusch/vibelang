//! System SynthDef for audio bus routing.
//!
//! This module builds the `system_link_audio` synthdef used for group bus routing.
//! The synthdef is built using direct binary encoding for efficiency.
//!
//! ## Signal Flow
//!
//! ```text
//! In.ar(inbus) → × amp → Out.ar(outbus)
//!                  ↓
//!           Peak + Amplitude → SendTrig (at 20Hz)
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
/// This synthdef routes audio from one bus to another with amplitude control
/// and metering. It's used for group bus routing in the VibeLang mixer.
///
/// # Parameters
///
/// - `inbus`: Input bus number (default: 0)
/// - `outbus`: Output bus number (default: 0)
/// - `amp`: Amplitude multiplier (default: 1.0)
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
    // 0: 0.0 (SendTrig ID 0 for peak_left)
    // 1: 1.0 (SendTrig ID 1 for peak_right)
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

    // Parameters: inbus=0, outbus=0, amp=1.0
    buf.write_all(&3i32.to_be_bytes())?; // num params
    buf.write_all(&0.0f32.to_be_bytes())?; // inbus default = 0
    buf.write_all(&0.0f32.to_be_bytes())?; // outbus default = 0
    buf.write_all(&1.0f32.to_be_bytes())?; // amp default = 1.0

    // Param names
    buf.write_all(&3i32.to_be_bytes())?; // num param names
    // inbus
    let inbus_name = b"inbus";
    buf.push(inbus_name.len() as u8);
    buf.write_all(inbus_name)?;
    buf.write_all(&0i32.to_be_bytes())?; // index 0
    // outbus
    let outbus_name = b"outbus";
    buf.push(outbus_name.len() as u8);
    buf.write_all(outbus_name)?;
    buf.write_all(&1i32.to_be_bytes())?; // index 1
    // amp
    let amp_name = b"amp";
    buf.push(amp_name.len() as u8);
    buf.write_all(amp_name)?;
    buf.write_all(&2i32.to_be_bytes())?; // index 2

    // UGens (14 total)
    buf.write_all(&14i32.to_be_bytes())?; // num ugens

    // UGen 0: Control (control rate, 3 outputs: inbus, outbus, amp)
    let control_name = b"Control";
    buf.push(control_name.len() as u8);
    buf.write_all(control_name)?;
    buf.push(1); // rate: control
    buf.write_all(&0i32.to_be_bytes())?; // num inputs
    buf.write_all(&3i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    buf.push(1); // output 0 rate: control (inbus)
    buf.push(1); // output 1 rate: control (outbus)
    buf.push(1); // output 2 rate: control (amp)

    // UGen 1: In.ar (audio rate, 2 outputs: left, right)
    let in_name = b"In";
    buf.push(in_name.len() as u8);
    buf.write_all(in_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&1i32.to_be_bytes())?; // num inputs
    buf.write_all(&2i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 0)?; // input: Control output 0 (inbus)
    buf.push(2); // output 0 rate: audio (left)
    buf.push(2); // output 1 rate: audio (right)

    // UGen 2: BinaryOpUGen * (left × amp) - audio rate
    let binop_name = b"BinaryOpUGen";
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special index: 2 = multiplication
    write_ugen_input(&mut buf, 1, 0)?; // input 0: In output 0 (left)
    write_ugen_input(&mut buf, 0, 2)?; // input 1: Control output 2 (amp)
    buf.push(2); // output rate: audio

    // UGen 3: BinaryOpUGen * (right × amp) - audio rate
    buf.push(binop_name.len() as u8);
    buf.write_all(binop_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&2i16.to_be_bytes())?; // special index: 2 = multiplication
    write_ugen_input(&mut buf, 1, 1)?; // input 0: In output 1 (right)
    write_ugen_input(&mut buf, 0, 2)?; // input 1: Control output 2 (amp)
    buf.push(2); // output rate: audio

    // UGen 4: Out.ar (outputs scaled audio to outbus)
    let out_name = b"Out";
    buf.push(out_name.len() as u8);
    buf.write_all(out_name)?;
    buf.push(2); // rate: audio
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 0, 1)?; // input 0: Control output 1 (outbus)
    write_ugen_input(&mut buf, 2, 0)?; // input 1: BinaryOpUGen#2 (scaled_left)
    write_ugen_input(&mut buf, 3, 0)?; // input 2: BinaryOpUGen#3 (scaled_right)

    // UGen 5: Impulse.kr (20Hz trigger for meter updates)
    let impulse_name = b"Impulse";
    buf.push(impulse_name.len() as u8);
    buf.write_all(impulse_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs (freq, phase)
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_const_input(&mut buf, 4)?; // input 0: constant 4 (20.0 Hz)
    write_const_input(&mut buf, 0)?; // input 1: constant 0 (0.0 phase)
    buf.push(1); // output rate: control

    // UGen 6: Peak.kr (left channel peak, reset by Impulse)
    let peak_name = b"Peak";
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 2, 0)?; // input 0: BinaryOpUGen#2 (scaled_left)
    write_ugen_input(&mut buf, 5, 0)?; // input 1: Impulse#5 (reset trigger)
    buf.push(1); // output rate: control

    // UGen 7: Peak.kr (right channel peak, reset by Impulse)
    buf.push(peak_name.len() as u8);
    buf.write_all(peak_name)?;
    buf.push(1); // rate: control
    buf.write_all(&2i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 3, 0)?; // input 0: BinaryOpUGen#3 (scaled_right)
    write_ugen_input(&mut buf, 5, 0)?; // input 1: Impulse#5 (reset trigger)
    buf.push(1); // output rate: control

    // UGen 8: Amplitude.kr (left channel RMS-like)
    let amplitude_name = b"Amplitude";
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 2, 0)?; // input 0: BinaryOpUGen#2 (scaled_left)
    write_const_input(&mut buf, 5)?; // input 1: constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // input 2: constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 9: Amplitude.kr (right channel RMS-like)
    buf.push(amplitude_name.len() as u8);
    buf.write_all(amplitude_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&1i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 3, 0)?; // input 0: BinaryOpUGen#3 (scaled_right)
    write_const_input(&mut buf, 5)?; // input 1: constant 5 (0.01 attack)
    write_const_input(&mut buf, 6)?; // input 2: constant 6 (0.1 release)
    buf.push(1); // output rate: control

    // UGen 10: SendTrig.kr (send peak_left)
    let sendtrig_name = b"SendTrig";
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 0)?; // input 1: constant 0 (ID = 0 for peak_left)
    write_ugen_input(&mut buf, 6, 0)?; // input 2: Peak#6 (peak_left value)

    // UGen 11: SendTrig.kr (send peak_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 1)?; // input 1: constant 1 (ID = 1 for peak_right)
    write_ugen_input(&mut buf, 7, 0)?; // input 2: Peak#7 (peak_right value)

    // UGen 12: SendTrig.kr (send rms_left)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 2)?; // input 1: constant 2 (ID = 2 for rms_left)
    write_ugen_input(&mut buf, 8, 0)?; // input 2: Amplitude#8 (rms_left value)

    // UGen 13: SendTrig.kr (send rms_right)
    buf.push(sendtrig_name.len() as u8);
    buf.write_all(sendtrig_name)?;
    buf.push(1); // rate: control
    buf.write_all(&3i32.to_be_bytes())?; // num inputs
    buf.write_all(&0i32.to_be_bytes())?; // num outputs
    buf.write_all(&0i16.to_be_bytes())?; // special index
    write_ugen_input(&mut buf, 5, 0)?; // input 0: Impulse#5 (trigger)
    write_const_input(&mut buf, 3)?; // input 1: constant 3 (ID = 3 for rms_right)
    write_ugen_input(&mut buf, 9, 0)?; // input 2: Amplitude#9 (rms_right value)

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
