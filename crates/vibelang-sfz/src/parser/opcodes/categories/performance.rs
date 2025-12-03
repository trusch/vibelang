use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for performance opcodes that control playback parameters
///
/// Performance opcodes control the real-time behavior of samples during playback,
/// including volume, tuning, pitch modulation, and various effects. These opcodes
/// are critical for creating expressive and dynamic instruments.
pub trait PerformanceOpcodes {
    // Basic parameters
    /// Gets the amplitude value (volume)
    ///
    /// Controls the overall volume of the region, specified in decibels.
    /// Positive values increase volume, negative values decrease it.
    ///
    /// Example: `volume=-6` (6 dB reduction in volume)
    fn volume(&self) -> Result<f32>;
    fn pan(&self) -> Result<f32>;
    fn width(&self) -> Result<f32>;
    fn position(&self) -> Result<f32>;
    fn amp_veltrack(&self) -> Result<f32>;
    fn amp_velcurve_(&self, vel: i32) -> Result<f32>;
    fn amp_random(&self) -> Result<f32>;
    fn xf_velcurve(&self) -> Result<String>;
    fn output(&self) -> Result<i32>;
    fn gain_cc(&self, cc: i32) -> Result<f32>;
    fn xfin_gain(&self) -> Result<f32>;
    fn xfout_gain(&self) -> Result<f32>;
    fn amplitude(&self) -> Result<f32>;
    fn amplitude_oncc(&self, cc: i32) -> Result<f32>;
    fn amplitude_smoothcc(&self, cc: i32) -> Result<i32>;
    #[allow(non_snake_case)]
    fn amplitude_curveccN(&self, cc: i32) -> Result<i32>;
    fn global_amplitude(&self) -> Result<f32>;
    fn master_amplitude(&self) -> Result<f32>;
    fn group_amplitude(&self) -> Result<f32>;
    fn note_gain(&self) -> Result<f32>;
    fn note_gain_oncc(&self, cc: i32) -> Result<f32>;
    fn volume_oncc(&self, cc: i32) -> Result<f32>;
    fn gain_oncc(&self, cc: i32) -> Result<f32>;
    fn global_volume(&self) -> Result<f32>;
    fn master_volume(&self) -> Result<f32>;
    fn group_volume(&self) -> Result<f32>;
    fn volume_cc(&self, cc: i32) -> Result<f32>;
    fn volume_smoothcc(&self, cc: i32) -> Result<i32>;
    fn volume_curvecca(&self, cc: i32) -> Result<i32>;
    
    // Panning
    fn pan_cc(&self, cc: i32) -> Result<f32>;
    fn pan_oncc(&self, cc: i32) -> Result<f32>;
    fn pan_smoothcc(&self, cc: i32) -> Result<i32>;
    fn pan_curvecca(&self, cc: i32) -> Result<i32>;
    fn pan_law(&self) -> Result<String>;
    fn position_oncc(&self, cc: i32) -> Result<f32>;
    fn width_oncc(&self, cc: i32) -> Result<f32>;
    
    // Pitch
    fn transpose(&self) -> Result<i32>;
    fn tune(&self) -> Result<i32>;
    fn pitch_keycenter(&self) -> Result<i32>;
    fn pitch_keytrack(&self) -> Result<i32>;
    fn pitch_veltrack(&self) -> Result<i32>;
    fn pitch_random(&self) -> Result<i32>;
    fn bend_up(&self) -> Result<i32>;
    fn bend_down(&self) -> Result<i32>;
    fn bend_step(&self) -> Result<i32>;
    fn pitch(&self) -> Result<f32>;
    fn pitch_oncc(&self, cc: i32) -> Result<f32>;
    fn pitch_smoothcc(&self, cc: i32) -> Result<i32>;
    fn pitch_curvecca(&self, cc: i32) -> Result<i32>;
    fn bend_smooth(&self) -> Result<i32>;
    fn bend_stepup(&self) -> Result<i32>;
    fn bend_stepdown(&self) -> Result<i32>;
    
    // Note articulation
    fn off_mode(&self) -> Result<String>;
    fn off_time(&self) -> Result<f32>;
    fn off_shape(&self) -> Result<String>;
    fn rt_decay(&self) -> Result<f32>;
    fn rt_dead(&self) -> Result<f32>;
    fn sw_vel(&self) -> Result<String>;
    fn voice_cap(&self) -> Result<i32>;
    fn voice_fader(&self) -> Result<f32>;
    fn voice_fader_oncc(&self, cc: i32) -> Result<f32>;
    fn voice_fader_smoothcc(&self, cc: i32) -> Result<i32>;
    fn voice_fader_curvecca(&self, cc: i32) -> Result<i32>;
    fn polyphony(&self) -> Result<i32>;
    fn polyphony_group(&self) -> Result<i32>;
    fn note_polyphony(&self) -> Result<i32>;
    fn note_selfmask(&self) -> Result<String>;
    fn sustain_sw(&self) -> Result<String>;
    
    // Channel routing
    fn delay_cc(&self, cc: i32) -> Result<f32>;
    fn offset_cc(&self, cc: i32) -> Result<i32>;
    fn delay_random_cc(&self, cc: i32) -> Result<f32>;
    fn offset_random_cc(&self, cc: i32) -> Result<i32>;
    
    // Performance control
    fn sustain_cc(&self, cc: i32) -> Result<i32>;
    fn sostenuto_cc(&self, cc: i32) -> Result<i32>;
    fn sostenuto_lo(&self) -> Result<i32>;
    fn sostenuto_hi(&self) -> Result<i32>;
}

impl PerformanceOpcodes for SfzSection {
    // Basic parameters
    fn volume(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "volume")
    }
    
    fn pan(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pan")
    }
    
    fn width(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "width")
    }
    
    fn position(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "position")
    }
    
    fn amp_veltrack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "amp_veltrack")
    }
    
    fn amp_velcurve_(&self, vel: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("amp_velcurve_{}", vel))
    }
    
    fn amp_random(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "amp_random")
    }
    
    fn xf_velcurve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "xf_velcurve")
    }
    
    fn output(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "output")
    }
    
    fn gain_cc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("gain_cc{}", cc))
    }
    
    fn xfin_gain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "xfin_gain")
    }
    
    fn xfout_gain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "xfout_gain")
    }
    
    fn amplitude(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "amplitude")
    }
    
    fn amplitude_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("amplitude_oncc{}", cc))
    }
    
    fn amplitude_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("amplitude_smoothcc{}", cc))
    }
    
    fn amplitude_curveccN(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("amplitude_curveccN{}", cc))
    }
    
    fn global_amplitude(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "global_amplitude")
    }
    
    fn master_amplitude(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "master_amplitude")
    }
    
    fn group_amplitude(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "group_amplitude")
    }
    
    fn note_gain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "note_gain")
    }
    
    fn note_gain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("note_gain_oncc{}", cc))
    }
    
    fn volume_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("volume_oncc{}", cc))
    }
    
    fn gain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("gain_oncc{}", cc))
    }
    
    fn global_volume(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "global_volume")
    }
    
    fn master_volume(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "master_volume")
    }
    
    fn group_volume(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "group_volume")
    }
    
    fn volume_cc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("volume_cc{}", cc))
    }
    
    fn volume_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("volume_smoothcc{}", cc))
    }
    
    fn volume_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("volume_curvecca{}", cc))
    }
    
    // Panning
    fn pan_cc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pan_cc{}", cc))
    }
    
    fn pan_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pan_oncc{}", cc))
    }
    
    fn pan_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("pan_smoothcc{}", cc))
    }
    
    fn pan_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("pan_curvecca{}", cc))
    }
    
    fn pan_law(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "pan_law")
    }
    
    fn position_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("position_oncc{}", cc))
    }
    
    fn width_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("width_oncc{}", cc))
    }
    
    // Pitch
    fn transpose(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "transpose")
    }
    
    fn tune(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "tune")
    }
    
    fn pitch_keycenter(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "pitch_keycenter")
    }
    
    fn pitch_keytrack(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "pitch_keytrack")
    }
    
    fn pitch_veltrack(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "pitch_veltrack")
    }
    
    fn pitch_random(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "pitch_random")
    }
    
    fn bend_up(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_up")
    }
    
    fn bend_down(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_down")
    }
    
    fn bend_step(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_step")
    }
    
    fn pitch(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitch")
    }
    
    fn pitch_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitch_oncc{}", cc))
    }
    
    fn pitch_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("pitch_smoothcc{}", cc))
    }
    
    fn pitch_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("pitch_curvecca{}", cc))
    }
    
    fn bend_smooth(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_smooth")
    }
    
    fn bend_stepup(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_stepup")
    }
    
    fn bend_stepdown(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "bend_stepdown")
    }
    
    // Note articulation
    fn off_mode(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "off_mode")
    }
    
    fn off_time(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "off_time")
    }
    
    fn off_shape(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "off_shape")
    }
    
    fn rt_decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "rt_decay")
    }
    
    fn rt_dead(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "rt_dead")
    }
    
    fn sw_vel(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "sw_vel")
    }
    
    fn voice_cap(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "voice_cap")
    }
    
    fn voice_fader(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "voice_fader")
    }
    
    fn voice_fader_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("voice_fader_oncc{}", cc))
    }
    
    fn voice_fader_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("voice_fader_smoothcc{}", cc))
    }
    
    fn voice_fader_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("voice_fader_curvecca{}", cc))
    }
    
    fn polyphony(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "polyphony")
    }
    
    fn polyphony_group(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "polyphony_group")
    }
    
    fn note_polyphony(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "note_polyphony")
    }
    
    fn note_selfmask(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "note_selfmask")
    }
    
    fn sustain_sw(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "sustain_sw")
    }
    
    // Channel routing
    fn delay_cc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("delay_cc{}", cc))
    }
    
    fn offset_cc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("offset_cc{}", cc))
    }
    
    fn delay_random_cc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("delay_random_cc{}", cc))
    }
    
    fn offset_random_cc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("offset_random_cc{}", cc))
    }
    
    // Performance control
    fn sustain_cc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("sustain_cc{}", cc))
    }
    
    fn sostenuto_cc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("sostenuto_cc{}", cc))
    }
    
    fn sostenuto_lo(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sostenuto_lo")
    }
    
    fn sostenuto_hi(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sostenuto_hi")
    }
} 