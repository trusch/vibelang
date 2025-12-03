use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for amplitude envelope opcodes
///
/// Amplitude envelopes control how a sound's volume changes over time, from the initial attack
/// when a note is triggered, through sustain while the note is held, and finally release when
/// the note ends. These opcodes allow for precise control over the ADSR (Attack, Decay, Sustain, Release)
/// envelope parameters, enabling realistic instrument dynamics and expressive performance.
pub trait AmplitudeEnvelopeOpcodes {
    /// Gets the attack time in seconds
    ///
    /// Specifies how long it takes for the sound to reach full volume after note-on.
    /// Shorter values create more percussive sounds, longer values create softer, fade-in effects.
    ///
    /// Example: `ampeg_attack=0.01` (10 millisecond attack time)
    fn ampeg_attack(&self) -> Result<f32>;
    
    /// Gets the attack time modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the attack time, in seconds.
    /// Positive values increase the attack time, negative values decrease it.
    ///
    /// Example: `ampeg_attack_oncc74=0.5` (CC 74 increases attack time up to 0.5 seconds)
    fn ampeg_attack_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the attack curve shape
    ///
    /// Controls the shape of the attack curve. Default is linear (0).
    /// Positive values create convex shapes, negative values create concave shapes.
    ///
    /// Example: `ampeg_attackcc1=0.3` (Slightly convex attack curve)
    fn ampeg_attackcc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the attack velocity sensitivity
    ///
    /// Controls how velocity affects the attack time. Positive values make
    /// higher velocities have shorter attack times (more immediacy).
    ///
    /// Example: `ampeg_veltrack_attack=-30` (Higher velocities have 30% shorter attack times)
    fn ampeg_veltrack_attack(&self) -> Result<f32>;
    
    /// Gets the attack velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_attack. Controls how velocity affects the attack time.
    /// Higher velocities result in shorter attack times for positive values.
    ///
    /// Example: `ampeg_vel2attack=-20` (Higher velocities have 20% shorter attack times)
    fn ampeg_vel2attack(&self) -> Result<f32>;
    
    /// Gets the attack shape parameter
    ///
    /// Defines the mathematical shape of the attack curve.
    /// Different values create different curve characteristics.
    ///
    /// Example: `ampeg_attack_shape=2.0` (Curved attack profile)
    fn ampeg_attack_shape(&self) -> Result<f32>;
    
    /// Gets the attack curve type
    ///
    /// Specifies the type of curve to use for the attack phase.
    /// Common values include "linear", "exponential", or "logarithmic".
    ///
    /// Example: `ampeg_attack_curve=exponential` (Exponential attack curve)
    fn ampeg_attack_curve(&self) -> Result<String>;
    
    /// Gets the decay time in seconds
    ///
    /// Specifies how long it takes for the sound to transition from the peak
    /// of the attack phase to the sustain level.
    ///
    /// Example: `ampeg_decay=1.2` (1.2 second decay time)
    fn ampeg_decay(&self) -> Result<f32>;
    
    /// Gets the decay time modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the decay time, in seconds.
    ///
    /// Example: `ampeg_decay_oncc70=2.0` (CC 70 increases decay time up to 2 seconds)
    fn ampeg_decay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the decay curve shape
    ///
    /// Controls the shape of the decay curve. Default is often slightly concave.
    /// Positive values create more convex shapes, negative values create more concave shapes.
    ///
    /// Example: `ampeg_decaycc1=-0.2` (Slightly more concave decay curve)
    fn ampeg_decaycc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the decay velocity sensitivity
    ///
    /// Controls how velocity affects the decay time. Positive values make
    /// higher velocities have longer decay times (sustained sound).
    ///
    /// Example: `ampeg_veltrack_decay=20` (Higher velocities have 20% longer decay times)
    fn ampeg_veltrack_decay(&self) -> Result<f32>;
    
    /// Gets the decay velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_decay. Controls how velocity affects the decay time.
    /// Higher velocities result in longer decay times for positive values.
    ///
    /// Example: `ampeg_vel2decay=15` (Higher velocities have 15% longer decay times)
    fn ampeg_vel2decay(&self) -> Result<f32>;
    
    /// Gets the decay shape parameter
    ///
    /// Defines the mathematical shape of the decay curve.
    /// Different values create different curve characteristics.
    ///
    /// Example: `ampeg_decay_shape=1.5` (Curved decay profile)
    fn ampeg_decay_shape(&self) -> Result<f32>;
    
    /// Gets the decay curve type
    ///
    /// Specifies the type of curve to use for the decay phase.
    /// Common values include "linear", "exponential", or "logarithmic".
    ///
    /// Example: `ampeg_decay_curve=logarithmic` (Logarithmic decay curve)
    fn ampeg_decay_curve(&self) -> Result<String>;
    
    /// Gets the delay time in seconds
    ///
    /// Specifies how long to wait before the envelope begins after note-on.
    /// Creates delayed entries or allows for layered timing effects.
    ///
    /// Example: `ampeg_delay=0.5` (0.5 second delay before envelope starts)
    fn ampeg_delay(&self) -> Result<f32>;
    
    /// Gets the delay time modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the delay time, in seconds.
    ///
    /// Example: `ampeg_delay_oncc72=1.0` (CC 72 increases delay time up to 1 second)
    fn ampeg_delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the delay velocity sensitivity
    ///
    /// Controls how velocity affects the delay time. Positive values make
    /// higher velocities have shorter delay times (quicker response).
    ///
    /// Example: `ampeg_veltrack_delay=-50` (Higher velocities have 50% shorter delay times)
    fn ampeg_veltrack_delay(&self) -> Result<f32>;
    
    /// Gets the delay velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_delay. Controls how velocity affects the delay time.
    /// Higher velocities result in shorter delay times for negative values.
    ///
    /// Example: `ampeg_vel2delay=-30` (Higher velocities have 30% shorter delay times)
    fn ampeg_vel2delay(&self) -> Result<f32>;
    
    /// Gets the delay shape parameter
    ///
    /// Defines the mathematical shape of the delay curve.
    /// Different values create different curve characteristics.
    ///
    /// Example: `ampeg_delay_shape=1.0` (Linear delay profile)
    fn ampeg_delay_shape(&self) -> Result<f32>;
    
    /// Gets the delay curve type
    ///
    /// Specifies the type of curve to use for the delay phase.
    /// Common values include "linear", "exponential", or "logarithmic".
    ///
    /// Example: `ampeg_delay_curve=linear` (Linear delay curve)
    fn ampeg_delay_curve(&self) -> Result<String>;
    
    /// Gets the hold time in seconds
    ///
    /// Specifies how long the envelope stays at peak level after the attack phase
    /// before beginning the decay phase.
    ///
    /// Example: `ampeg_hold=0.2` (0.2 second hold time at peak volume)
    fn ampeg_hold(&self) -> Result<f32>;
    
    /// Gets the hold time modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the hold time, in seconds.
    ///
    /// Example: `ampeg_hold_oncc71=0.5` (CC 71 increases hold time up to 0.5 seconds)
    fn ampeg_hold_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the hold velocity sensitivity
    ///
    /// Controls how velocity affects the hold time. Positive values make
    /// higher velocities have longer hold times (sustained peak).
    ///
    /// Example: `ampeg_veltrack_hold=40` (Higher velocities have 40% longer hold times)
    fn ampeg_veltrack_hold(&self) -> Result<f32>;
    
    /// Gets the hold velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_hold. Controls how velocity affects the hold time.
    /// Higher velocities result in longer hold times for positive values.
    ///
    /// Example: `ampeg_vel2hold=25` (Higher velocities have 25% longer hold times)
    fn ampeg_vel2hold(&self) -> Result<f32>;
    
    /// Gets the hold shape parameter
    ///
    /// Defines the mathematical shape of the hold curve.
    /// Different values create different curve characteristics.
    ///
    /// Example: `ampeg_hold_shape=0.0` (Flat hold profile)
    fn ampeg_hold_shape(&self) -> Result<f32>;
    
    /// Gets the hold curve type
    ///
    /// Specifies the type of curve to use for the hold phase.
    /// Common values include "linear", "exponential", or "logarithmic".
    ///
    /// Example: `ampeg_hold_curve=flat` (Flat hold curve)
    fn ampeg_hold_curve(&self) -> Result<String>;
    
    /// Gets the release time in seconds
    ///
    /// Specifies how long it takes for the sound to fade out after note-off.
    /// Critical for natural sounding note endings.
    ///
    /// Example: `ampeg_release=0.3` (0.3 second release time)
    fn ampeg_release(&self) -> Result<f32>;
    
    /// Gets the release time modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the release time, in seconds.
    ///
    /// Example: `ampeg_release_oncc72=2.0` (CC 72 increases release time up to 2 seconds)
    fn ampeg_release_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the release curve shape
    ///
    /// Controls the shape of the release curve. Default is often slightly concave.
    /// Positive values create more convex shapes, negative values create more concave shapes.
    ///
    /// Example: `ampeg_releasecc1=-0.3` (More concave release curve)
    fn ampeg_releasecc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the release velocity sensitivity
    ///
    /// Controls how velocity affects the release time. Positive values make
    /// higher velocities have longer release times (longer decay).
    ///
    /// Example: `ampeg_veltrack_release=20` (Higher velocities have 20% longer release times)
    fn ampeg_veltrack_release(&self) -> Result<f32>;
    
    /// Gets the release velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_release. Controls how velocity affects the release time.
    /// Higher velocities result in longer release times for positive values.
    ///
    /// Example: `ampeg_vel2release=15` (Higher velocities have 15% longer release times)
    fn ampeg_vel2release(&self) -> Result<f32>;
    
    /// Gets the release shape parameter
    ///
    /// Defines the mathematical shape of the release curve.
    /// Different values create different curve characteristics.
    ///
    /// Example: `ampeg_release_shape=2.5` (Curved release profile)
    fn ampeg_release_shape(&self) -> Result<f32>;
    
    /// Gets the release curve type
    ///
    /// Specifies the type of curve to use for the release phase.
    /// Common values include "linear", "exponential", or "logarithmic".
    ///
    /// Example: `ampeg_release_curve=exponential` (Exponential release curve)
    fn ampeg_release_curve(&self) -> Result<String>;
    
    /// Gets the sustain level as a percentage
    ///
    /// Specifies the level at which the sound is held during the sustain phase.
    /// 100 is full volume, 0 is silence.
    ///
    /// Example: `ampeg_sustain=60` (Sustain at 60% of peak volume)
    fn ampeg_sustain(&self) -> Result<f32>;
    
    /// Gets the sustain level modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the sustain level, in percentage.
    ///
    /// Example: `ampeg_sustain_oncc73=50` (CC 73 increases sustain level up to 50%)
    fn ampeg_sustain_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the sustain velocity sensitivity
    ///
    /// Controls how velocity affects the sustain level. Positive values make
    /// higher velocities have higher sustain levels (louder sustained notes).
    ///
    /// Example: `ampeg_veltrack_sustain=30` (Higher velocities have 30% higher sustain levels)
    fn ampeg_veltrack_sustain(&self) -> Result<f32>;
    
    /// Gets the sustain velocity sensitivity
    ///
    /// Alternative to ampeg_veltrack_sustain. Controls how velocity affects the sustain level.
    /// Higher velocities result in higher sustain levels for positive values.
    ///
    /// Example: `ampeg_vel2sustain=20` (Higher velocities have 20% higher sustain levels)
    fn ampeg_vel2sustain(&self) -> Result<f32>;
    
    /// Gets the start level of the amplitude envelope
    ///
    /// Specifies the initial level of the envelope before attack phase begins.
    /// 0 is silence, 100 is full volume.
    ///
    /// Example: `ampeg_start=10` (Start at 10% of full volume)
    fn ampeg_start(&self) -> Result<f32>;
    
    /// Gets the start level modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the start level, in percentage.
    ///
    /// Example: `ampeg_start_oncc20=30` (CC 20 increases start level up to 30%)
    fn ampeg_start_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the depth of the amplitude envelope
    ///
    /// Controls how much the envelope affects the overall amplitude.
    /// 100% means full envelope application, 0% means no envelope effect.
    ///
    /// Example: `ampeg_depth=80` (Envelope affects 80% of the amplitude)
    fn ampeg_depth(&self) -> Result<f32>;
    
    /// Gets the amplitude envelope depth modulation from MIDI CC
    ///
    /// Specifies how much a MIDI CC affects the envelope depth, in percentage.
    ///
    /// Example: `ampeg_depth_oncc11=50` (Expression CC modulates envelope depth up to 50%)
    fn ampeg_depth_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the amplitude envelope
    ///
    /// Controls how velocity affects the overall envelope intensity.
    /// 100% means full velocity sensitivity, 0% means no velocity effect.
    ///
    /// Example: `ampeg_vel2depth=60` (Velocity affects envelope depth by 60%)
    fn ampeg_vel2depth(&self) -> Result<f32>;
    
    /// Gets the velocity curve of the amplitude envelope
    ///
    /// Defines the shape of the velocity response curve for the envelope.
    /// Different values create different velocity response characteristics.
    ///
    /// Example: `ampeg_vel2curve=2` (Use velocity curve 2 for envelope response)
    fn ampeg_vel2curve(&self) -> Result<i32>;
    
    /// Gets the amplitude envelope dynamic range
    ///
    /// Specifies the dynamic range of the envelope in decibels.
    /// Controls how much the volume can be reduced during envelope stages.
    ///
    /// Example: `ampeg_dynamic=60` (Envelope has a dynamic range of 60 dB)
    fn ampeg_dynamic(&self) -> Result<f32>;
    
    /// Gets the key tracking for amplitude envelope parameters
    ///
    /// Controls how the envelope parameters change across the keyboard range.
    /// Higher values make higher notes have faster envelope stages.
    ///
    /// Example: `ampeg_keytrackN=0.5` (Envelope speeds up slightly for higher notes)
    #[allow(non_snake_case)]
    fn ampeg_keytrackerN(&self, n: i32) -> Result<f32>;
    
    /// Gets the amplitude LFO to envelope modulation amount
    ///
    /// Controls how much the LFO affects the amplitude envelope parameters.
    /// Creates dynamic modulation of envelope parameters.
    ///
    /// Example: `amplfo_depthN=10` (LFO modulates envelope parameter N by 10%)
    #[allow(non_snake_case)]
    fn amplfo_depthN(&self, n: i32) -> Result<f32>;
    
    /// Gets whether the amplitude envelope dynamic behavior is enabled
    ///
    /// When enabled, the envelope responds dynamically to changes in MIDI controllers.
    /// When disabled, envelope only reacts at note-on time.
    ///
    /// Example: `ampeg_dynamic_enabled=1` (Enable dynamic envelope behavior)
    fn ampeg_dynamic_enabled(&self) -> Result<i32>;
}

impl AmplitudeEnvelopeOpcodes for SfzSection {
    fn ampeg_attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_attack")
    }
    
    fn ampeg_decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_decay")
    }
    
    fn ampeg_delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_delay")
    }
    
    fn ampeg_hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_hold")
    }
    
    fn ampeg_release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_release")
    }
    
    fn ampeg_start(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_start")
    }
    
    fn ampeg_sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_sustain")
    }
    
    fn ampeg_vel2attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2attack")
    }
    
    fn ampeg_vel2decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2decay")
    }
    
    fn ampeg_vel2delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2delay")
    }
    
    fn ampeg_vel2hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2hold")
    }
    
    fn ampeg_vel2release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2release")
    }
    
    fn ampeg_vel2sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2sustain")
    }
    
    fn ampeg_attackcc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_attackcc{}", cc))
    }
    
    fn ampeg_veltrack_attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_attack")
    }
    
    fn ampeg_decaycc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_decaycc{}", cc))
    }
    
    fn ampeg_veltrack_decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_decay")
    }
    
    fn ampeg_veltrack_delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_delay")
    }
    
    fn ampeg_veltrack_hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_hold")
    }
    
    fn ampeg_releasecc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_releasecc{}", cc))
    }
    
    fn ampeg_veltrack_release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_release")
    }
    
    fn ampeg_veltrack_sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_veltrack_sustain")
    }
    
    fn ampeg_attack_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_attack_shape")
    }
    
    fn ampeg_decay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_decay_shape")
    }
    
    fn ampeg_delay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_delay_shape")
    }
    
    fn ampeg_hold_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_hold_shape")
    }
    
    fn ampeg_release_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_release_shape")
    }
    
    fn ampeg_attack_curve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "ampeg_attack_curve")
    }
    
    fn ampeg_decay_curve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "ampeg_decay_curve")
    }
    
    fn ampeg_delay_curve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "ampeg_delay_curve")
    }
    
    fn ampeg_hold_curve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "ampeg_hold_curve")
    }
    
    fn ampeg_release_curve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "ampeg_release_curve")
    }
    
    fn ampeg_attack_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_attack_oncc{}", cc))
    }
    
    fn ampeg_decay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_decay_oncc{}", cc))
    }
    
    fn ampeg_delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_delay_oncc{}", cc))
    }
    
    fn ampeg_hold_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_hold_oncc{}", cc))
    }
    
    fn ampeg_release_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_release_oncc{}", cc))
    }
    
    fn ampeg_start_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_start_oncc{}", cc))
    }
    
    fn ampeg_sustain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_sustain_oncc{}", cc))
    }
    
    fn ampeg_depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_depth")
    }
    
    fn ampeg_depth_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_depth_oncc{}", cc))
    }
    
    fn ampeg_vel2depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2depth")
    }
    
    fn ampeg_vel2curve(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "ampeg_vel2curve")
    }
    
    fn ampeg_dynamic(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "ampeg_dynamic")
    }
    
    fn ampeg_keytrackerN(&self, n: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("ampeg_keytracker{}", n))
    }
    
    fn amplfo_depthN(&self, n: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("amplfo_depth{}", n))
    }
    
    fn ampeg_dynamic_enabled(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "ampeg_dynamic_enabled")
    }
} 