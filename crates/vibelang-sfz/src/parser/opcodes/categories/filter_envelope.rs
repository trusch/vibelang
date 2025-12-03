use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for filter envelope opcodes
///
/// Filter envelopes control how a filter's characteristics change over time.
/// They shape the timbre evolution of sounds by dynamically modifying filter 
/// parameters (primarily cutoff frequency) through the standard ADSR 
/// (Attack, Decay, Sustain, Release) envelope stages. Filter envelopes 
/// are essential for creating expressive and dynamic timbral changes in
/// synthesized and sampled sounds.
pub trait FilterEnvelopeOpcodes {
    /// Gets the filter envelope attack time in seconds
    ///
    /// Controls how long it takes for the filter modulation to reach its initial target value.
    /// Shorter times create more immediate timbral changes, longer times create gradual sweeps.
    ///
    /// Example: `fileg_attack=0.1` (Filter envelope takes 0.1 seconds to reach initial target)
    fn fileg_attack(&self) -> Result<f32>;
    
    /// Gets the filter envelope attack time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the attack time of the filter envelope.
    ///
    /// Example: `fileg_attack_oncc1=0.5` (Mod wheel increases attack time up to 0.5 seconds)
    fn fileg_attack_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter envelope attack curve shape
    ///
    /// Controls the shape of the attack phase of the filter envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `fileg_attack_shape=2.0` (Convex attack curve for filter sweep)
    fn fileg_attack_shape(&self) -> Result<f32>;
    
    /// Gets the filter envelope decay time in seconds
    ///
    /// Controls how long it takes for the filter to move from the attack level to the sustain level.
    /// Creates falling or rising filter movements after the initial attack.
    ///
    /// Example: `fileg_decay=0.3` (Filter takes 0.3 seconds to reach sustain level)
    fn fileg_decay(&self) -> Result<f32>;
    
    /// Gets the filter envelope decay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the decay time of the filter envelope.
    ///
    /// Example: `fileg_decay_oncc1=0.8` (Mod wheel increases decay time up to 0.8 seconds)
    fn fileg_decay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter envelope decay curve shape
    ///
    /// Controls the shape of the decay phase of the filter envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `fileg_decay_shape=-1.0` (Concave decay curve for filter movement)
    fn fileg_decay_shape(&self) -> Result<f32>;
    
    /// Gets the filter envelope delay time in seconds
    ///
    /// Controls how long to wait before starting the filter envelope.
    /// Useful for creating delayed filter effects.
    ///
    /// Example: `fileg_delay=0.2` (Filter envelope starts 0.2 seconds after note-on)
    fn fileg_delay(&self) -> Result<f32>;
    
    /// Gets the filter envelope delay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the delay time of the filter envelope.
    ///
    /// Example: `fileg_delay_oncc1=0.5` (Mod wheel increases delay time up to 0.5 seconds)
    fn fileg_delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter envelope delay curve shape
    ///
    /// Controls the shape of the delay phase of the filter envelope.
    /// Affects how filter changes during the delay phase.
    ///
    /// Example: `fileg_delay_shape=0.0` (Linear delay curve)
    fn fileg_delay_shape(&self) -> Result<f32>;
    
    /// Gets the filter envelope hold time in seconds
    ///
    /// Controls how long the filter stays at the peak level after the attack phase.
    /// Creates stable filter plateaus before the decay phase.
    ///
    /// Example: `fileg_hold=0.2` (Filter stays at peak for 0.2 seconds)
    fn fileg_hold(&self) -> Result<f32>;
    
    /// Gets the filter envelope hold time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the hold time of the filter envelope.
    ///
    /// Example: `fileg_hold_oncc1=0.4` (Mod wheel increases hold time up to 0.4 seconds)
    fn fileg_hold_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter envelope hold curve shape
    ///
    /// Controls the shape of the hold phase of the filter envelope.
    /// Affects the stability of the filter during the hold phase.
    ///
    /// Example: `fileg_hold_shape=0.0` (Flat hold curve)
    fn fileg_hold_shape(&self) -> Result<f32>;
    
    /// Gets the filter envelope release time in seconds
    ///
    /// Controls how long it takes for the filter modulation to return to normal after note-off.
    /// Creates filter slides at the end of notes.
    ///
    /// Example: `fileg_release=0.5` (Filter returns to normal in 0.5 seconds after note-off)
    fn fileg_release(&self) -> Result<f32>;
    
    /// Gets the filter envelope release time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the release time of the filter envelope.
    ///
    /// Example: `fileg_release_oncc1=1.0` (Mod wheel increases release time up to 1 second)
    fn fileg_release_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter envelope release curve shape
    ///
    /// Controls the shape of the release phase of the filter envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `fileg_release_shape=2.0` (Convex release curve for filter decay)
    fn fileg_release_shape(&self) -> Result<f32>;
    
    /// Gets the filter envelope sustain level
    ///
    /// Controls the filter offset during the sustain phase of the envelope, in cents.
    /// Positive values raise the cutoff, negative values lower it.
    ///
    /// Example: `fileg_sustain=50` (Sustain filter cutoff is 50 cents higher)
    fn fileg_sustain(&self) -> Result<f32>;
    
    /// Gets the filter envelope sustain level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the sustain level of the filter envelope.
    ///
    /// Example: `fileg_sustain_oncc1=100` (Mod wheel raises sustain level up to 100 cents)
    fn fileg_sustain_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the starting level of the filter envelope
    ///
    /// Controls the initial filter offset before the attack phase, in cents.
    /// Positive values start with higher cutoff, negative values start with lower cutoff.
    ///
    /// Example: `fileg_start=12` (Start with cutoff 12 cents above normal)
    fn fileg_start(&self) -> Result<f32>;
    
    /// Gets the filter envelope start level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the starting level of the filter envelope.
    ///
    /// Example: `fileg_start_oncc1=24` (Mod wheel raises starting cutoff up to 24 cents)
    fn fileg_start_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the depth of the filter envelope
    ///
    /// Controls the overall intensity of the filter envelope, in cents.
    /// Higher values create more dramatic filter changes.
    ///
    /// Example: `fileg_depth=2400` (Filter envelope range is 2400 cents or 2 octaves)
    fn fileg_depth(&self) -> Result<f32>;
    
    /// Gets the filter envelope depth modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the depth of the filter envelope.
    ///
    /// Example: `fileg_depth_oncc1=1200` (Mod wheel increases depth up to 1200 cents or 1 octave)
    fn fileg_depth_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope
    ///
    /// Controls how velocity affects the overall filter envelope intensity.
    /// Positive values make higher velocities have stronger filter envelope effects.
    ///
    /// Example: `fileg_vel2depth=600` (Velocity can add up to 600 cents to envelope depth)
    fn fileg_vel2depth(&self) -> Result<f32>;
    
    /// Gets the velocity curve for filter envelope
    ///
    /// Specifies which curve to use for velocity to envelope depth mapping.
    /// References a curve definition elsewhere in the SFZ file.
    ///
    /// Example: `fileg_vel2curve=2` (Use curve 2 for velocity to envelope depth mapping)
    fn fileg_vel2curve(&self) -> Result<i32>;
    
    /// Gets the velocity sensitivity of the filter envelope attack
    ///
    /// Controls how velocity affects the attack time of the filter envelope.
    /// Negative values make higher velocities have shorter attack times.
    ///
    /// Example: `fileg_vel2attack=-50` (Higher velocities have shorter attack times)
    fn fileg_vel2attack(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope decay
    ///
    /// Controls how velocity affects the decay time of the filter envelope.
    /// Positive values make higher velocities have longer decay times.
    ///
    /// Example: `fileg_vel2decay=30` (Higher velocities have longer decay times)
    fn fileg_vel2decay(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope delay
    ///
    /// Controls how velocity affects the delay time of the filter envelope.
    /// Negative values make higher velocities have shorter delay times.
    ///
    /// Example: `fileg_vel2delay=-20` (Higher velocities have shorter delay times)
    fn fileg_vel2delay(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope hold
    ///
    /// Controls how velocity affects the hold time of the filter envelope.
    /// Positive values make higher velocities have longer hold times.
    ///
    /// Example: `fileg_vel2hold=40` (Higher velocities have longer hold times)
    fn fileg_vel2hold(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope release
    ///
    /// Controls how velocity affects the release time of the filter envelope.
    /// Positive values make higher velocities have longer release times.
    ///
    /// Example: `fileg_vel2release=25` (Higher velocities have longer release times)
    fn fileg_vel2release(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the filter envelope sustain
    ///
    /// Controls how velocity affects the sustain level of the filter envelope.
    /// Positive values make higher velocities have higher sustain levels.
    ///
    /// Example: `fileg_vel2sustain=60` (Higher velocities have higher sustain levels)
    fn fileg_vel2sustain(&self) -> Result<f32>;
    
    /// Gets the dynamic control of the filter envelope
    ///
    /// When enabled, the envelope responds dynamically to changes in MIDI controllers.
    /// When disabled, envelope only reacts at note-on time.
    ///
    /// Example: `fileg_dynamic=1` (Enable dynamic filter envelope behavior)
    fn fileg_dynamic(&self) -> Result<i32>;
    
    /// Gets the velocity to attack time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the attack time.
    ///
    /// Example: `fileg_vel2attack_oncc1=25` (Mod wheel increases velocity sensitivity of attack)
    fn fileg_vel2attack_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to decay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the decay time.
    ///
    /// Example: `fileg_vel2decay_oncc1=20` (Mod wheel increases velocity sensitivity of decay)
    fn fileg_vel2decay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to delay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the delay time.
    ///
    /// Example: `fileg_vel2delay_oncc1=-15` (Mod wheel decreases velocity sensitivity of delay)
    fn fileg_vel2delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to hold time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the hold time.
    ///
    /// Example: `fileg_vel2hold_oncc1=30` (Mod wheel increases velocity sensitivity of hold)
    fn fileg_vel2hold_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to release time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the release time.
    ///
    /// Example: `fileg_vel2release_oncc1=15` (Mod wheel increases velocity sensitivity of release)
    fn fileg_vel2release_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to sustain level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the sustain level.
    ///
    /// Example: `fileg_vel2sustain_oncc1=50` (Mod wheel increases velocity sensitivity of sustain)
    fn fileg_vel2sustain_oncc(&self, cc: i32) -> Result<f32>;
}

impl FilterEnvelopeOpcodes for SfzSection {
    fn fileg_attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_attack")
    }
    
    fn fileg_decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_decay")
    }
    
    fn fileg_delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_delay")
    }
    
    fn fileg_hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_hold")
    }
    
    fn fileg_release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_release")
    }
    
    fn fileg_start(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_start")
    }
    
    fn fileg_sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_sustain")
    }
    
    fn fileg_depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_depth")
    }
    
    fn fileg_vel2attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2attack")
    }
    
    fn fileg_vel2decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2decay")
    }
    
    fn fileg_vel2delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2delay")
    }
    
    fn fileg_vel2hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2hold")
    }
    
    fn fileg_vel2release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2release")
    }
    
    fn fileg_vel2sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2sustain")
    }
    
    fn fileg_vel2depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2depth")
    }
    
    fn fileg_attack_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_attack_shape")
    }
    
    fn fileg_decay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_decay_shape")
    }
    
    fn fileg_delay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_delay_shape")
    }
    
    fn fileg_hold_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_hold_shape")
    }
    
    fn fileg_release_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fileg_release_shape")
    }
    
    fn fileg_attack_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_attack_oncc{}", cc))
    }
    
    fn fileg_decay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_decay_oncc{}", cc))
    }
    
    fn fileg_delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_delay_oncc{}", cc))
    }
    
    fn fileg_hold_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_hold_oncc{}", cc))
    }
    
    fn fileg_release_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_release_oncc{}", cc))
    }
    
    fn fileg_start_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_start_oncc{}", cc))
    }
    
    fn fileg_sustain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_sustain_oncc{}", cc))
    }
    
    fn fileg_depth_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_depth_oncc{}", cc))
    }
    
    fn fileg_vel2attack_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2attack_oncc{}", cc))
    }
    
    fn fileg_vel2decay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2decay_oncc{}", cc))
    }
    
    fn fileg_vel2delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2delay_oncc{}", cc))
    }
    
    fn fileg_vel2hold_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2hold_oncc{}", cc))
    }
    
    fn fileg_vel2release_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2release_oncc{}", cc))
    }
    
    fn fileg_vel2sustain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fileg_vel2sustain_oncc{}", cc))
    }
    
    fn fileg_vel2curve(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "fileg_vel2curve")
    }
    
    fn fileg_dynamic(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "fileg_dynamic")
    }
} 