use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for pitch envelope opcodes
///
/// Pitch envelopes control how the pitch of a sound changes over time, enabling
/// dynamic pitch modulation effects such as pitch sweeps, vibrato, and bends.
/// These opcodes allow for precise control over the envelope parameters that
/// shape pitch changes, creating more expressive and evolving sounds.
pub trait PitchEnvelopeOpcodes {
    /// Gets the pitch envelope attack time in seconds
    ///
    /// Controls how long it takes for the pitch modulation to reach its initial target value.
    /// Useful for creating gradual pitch slides at the start of notes.
    ///
    /// Example: `pitcheg_attack=0.1` (Pitch envelope takes 0.1 seconds to reach initial target)
    fn pitcheg_attack(&self) -> Result<f32>;
    
    /// Gets the pitch envelope attack time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the attack time of the pitch envelope.
    ///
    /// Example: `pitcheg_attack_oncc1=0.5` (Mod wheel increases attack time up to 0.5 seconds)
    fn pitcheg_attack_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the pitch envelope attack curve shape
    ///
    /// Controls the shape of the attack phase of the pitch envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `pitcheg_attack_shape=2.0` (Convex attack curve)
    fn pitcheg_attack_shape(&self) -> Result<f32>;
    
    /// Gets the pitch envelope decay time in seconds
    ///
    /// Controls how long it takes for the pitch to move from the attack level to the sustain level.
    /// Creates falling or rising pitch movements after the initial attack.
    ///
    /// Example: `pitcheg_decay=0.3` (Pitch takes 0.3 seconds to reach sustain level)
    fn pitcheg_decay(&self) -> Result<f32>;
    
    /// Gets the pitch envelope decay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the decay time of the pitch envelope.
    ///
    /// Example: `pitcheg_decay_oncc1=0.8` (Mod wheel increases decay time up to 0.8 seconds)
    fn pitcheg_decay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the pitch envelope decay curve shape
    ///
    /// Controls the shape of the decay phase of the pitch envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `pitcheg_decay_shape=-1.0` (Concave decay curve)
    fn pitcheg_decay_shape(&self) -> Result<f32>;
    
    /// Gets the pitch envelope delay time in seconds
    ///
    /// Controls how long to wait before starting the pitch envelope.
    /// Useful for creating delayed pitch effects.
    ///
    /// Example: `pitcheg_delay=0.2` (Pitch envelope starts 0.2 seconds after note-on)
    fn pitcheg_delay(&self) -> Result<f32>;
    
    /// Gets the pitch envelope delay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the delay time of the pitch envelope.
    ///
    /// Example: `pitcheg_delay_oncc1=0.5` (Mod wheel increases delay time up to 0.5 seconds)
    fn pitcheg_delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the pitch envelope delay curve shape
    ///
    /// Controls the shape of the delay phase of the pitch envelope.
    /// Affects how pitch changes during the delay phase.
    ///
    /// Example: `pitcheg_delay_shape=0.0` (Linear delay curve)
    fn pitcheg_delay_shape(&self) -> Result<f32>;
    
    /// Gets the pitch envelope hold time in seconds
    ///
    /// Controls how long the pitch stays at the peak level after the attack phase.
    /// Creates stable pitch plateaus before the decay phase.
    ///
    /// Example: `pitcheg_hold=0.2` (Pitch stays at peak for 0.2 seconds)
    fn pitcheg_hold(&self) -> Result<f32>;
    
    /// Gets the pitch envelope hold time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the hold time of the pitch envelope.
    ///
    /// Example: `pitcheg_hold_oncc1=0.4` (Mod wheel increases hold time up to 0.4 seconds)
    fn pitcheg_hold_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the pitch envelope hold curve shape
    ///
    /// Controls the shape of the hold phase of the pitch envelope.
    /// Affects the stability of the pitch during the hold phase.
    ///
    /// Example: `pitcheg_hold_shape=0.0` (Flat hold curve)
    fn pitcheg_hold_shape(&self) -> Result<f32>;
    
    /// Gets the pitch envelope release time in seconds
    ///
    /// Controls how long it takes for the pitch modulation to return to normal after note-off.
    /// Creates pitch slides at the end of notes.
    ///
    /// Example: `pitcheg_release=0.5` (Pitch returns to normal in 0.5 seconds after note-off)
    fn pitcheg_release(&self) -> Result<f32>;
    
    /// Gets the pitch envelope release time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the release time of the pitch envelope.
    ///
    /// Example: `pitcheg_release_oncc1=1.0` (Mod wheel increases release time up to 1 second)
    fn pitcheg_release_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the pitch envelope release curve shape
    ///
    /// Controls the shape of the release phase of the pitch envelope.
    /// Positive values create convex curves, negative values create concave curves.
    ///
    /// Example: `pitcheg_release_shape=2.0` (Convex release curve)
    fn pitcheg_release_shape(&self) -> Result<f32>;
    
    /// Gets the pitch envelope sustain level
    ///
    /// Controls the pitch offset during the sustain phase of the envelope, in cents.
    /// Positive values raise the pitch, negative values lower it.
    ///
    /// Example: `pitcheg_sustain=50` (Sustain pitch is 50 cents higher)
    fn pitcheg_sustain(&self) -> Result<f32>;
    
    /// Gets the pitch envelope sustain level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the sustain level of the pitch envelope.
    ///
    /// Example: `pitcheg_sustain_oncc1=100` (Mod wheel raises sustain level up to 100 cents)
    fn pitcheg_sustain_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the starting level of the pitch envelope
    ///
    /// Controls the initial pitch offset before the attack phase, in cents.
    /// Positive values start with higher pitch, negative values start with lower pitch.
    ///
    /// Example: `pitcheg_start=12` (Start 12 cents above normal pitch)
    fn pitcheg_start(&self) -> Result<f32>;
    
    /// Gets the pitch envelope start level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the starting level of the pitch envelope.
    ///
    /// Example: `pitcheg_start_oncc1=24` (Mod wheel raises starting pitch up to 24 cents)
    fn pitcheg_start_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the depth of the pitch envelope
    ///
    /// Controls the overall intensity of the pitch envelope, in cents.
    /// Higher values create more dramatic pitch changes.
    ///
    /// Example: `pitcheg_depth=1200` (Pitch envelope range is 1200 cents or 1 octave)
    fn pitcheg_depth(&self) -> Result<f32>;
    
    /// Gets the pitch envelope depth modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the depth of the pitch envelope.
    ///
    /// Example: `pitcheg_depth_oncc1=600` (Mod wheel increases depth up to 600 cents or half octave)
    fn pitcheg_depth_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope
    ///
    /// Controls how velocity affects the overall pitch envelope intensity.
    /// Positive values make higher velocities have stronger pitch envelope effects.
    ///
    /// Example: `pitcheg_vel2depth=300` (Velocity can add up to 300 cents to envelope depth)
    fn pitcheg_vel2depth(&self) -> Result<f32>;
    
    /// Gets the velocity curve for pitch envelope
    ///
    /// Specifies which curve to use for velocity to envelope depth mapping.
    /// References a curve definition elsewhere in the SFZ file.
    ///
    /// Example: `pitcheg_vel2curve=2` (Use curve 2 for velocity to envelope depth mapping)
    fn pitcheg_vel2curve(&self) -> Result<i32>;
    
    /// Gets the velocity sensitivity of the pitch envelope attack
    ///
    /// Controls how velocity affects the attack time of the pitch envelope.
    /// Negative values make higher velocities have shorter attack times.
    ///
    /// Example: `pitcheg_vel2attack=-50` (Higher velocities have shorter attack times)
    fn pitcheg_vel2attack(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope decay
    ///
    /// Controls how velocity affects the decay time of the pitch envelope.
    /// Positive values make higher velocities have longer decay times.
    ///
    /// Example: `pitcheg_vel2decay=30` (Higher velocities have longer decay times)
    fn pitcheg_vel2decay(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope delay
    ///
    /// Controls how velocity affects the delay time of the pitch envelope.
    /// Negative values make higher velocities have shorter delay times.
    ///
    /// Example: `pitcheg_vel2delay=-20` (Higher velocities have shorter delay times)
    fn pitcheg_vel2delay(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope hold
    ///
    /// Controls how velocity affects the hold time of the pitch envelope.
    /// Positive values make higher velocities have longer hold times.
    ///
    /// Example: `pitcheg_vel2hold=40` (Higher velocities have longer hold times)
    fn pitcheg_vel2hold(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope release
    ///
    /// Controls how velocity affects the release time of the pitch envelope.
    /// Positive values make higher velocities have longer release times.
    ///
    /// Example: `pitcheg_vel2release=25` (Higher velocities have longer release times)
    fn pitcheg_vel2release(&self) -> Result<f32>;
    
    /// Gets the velocity sensitivity of the pitch envelope sustain
    ///
    /// Controls how velocity affects the sustain level of the pitch envelope.
    /// Positive values make higher velocities have higher sustain levels.
    ///
    /// Example: `pitcheg_vel2sustain=60` (Higher velocities have higher sustain levels)
    fn pitcheg_vel2sustain(&self) -> Result<f32>;
    
    /// Gets the velocity to attack time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the attack time.
    ///
    /// Example: `pitcheg_vel2attack_oncc1=25` (Mod wheel increases velocity sensitivity of attack)
    fn pitcheg_vel2attack_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to decay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the decay time.
    ///
    /// Example: `pitcheg_vel2decay_oncc1=20` (Mod wheel increases velocity sensitivity of decay)
    fn pitcheg_vel2decay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to delay time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the delay time.
    ///
    /// Example: `pitcheg_vel2delay_oncc1=-15` (Mod wheel decreases velocity sensitivity of delay)
    fn pitcheg_vel2delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to hold time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the hold time.
    ///
    /// Example: `pitcheg_vel2hold_oncc1=30` (Mod wheel increases velocity sensitivity of hold)
    fn pitcheg_vel2hold_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to release time modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the release time.
    ///
    /// Example: `pitcheg_vel2release_oncc1=15` (Mod wheel increases velocity sensitivity of release)
    fn pitcheg_vel2release_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the velocity to sustain level modulation from MIDI CC
    ///
    /// Controls how a MIDI CC affects the velocity sensitivity of the sustain level.
    ///
    /// Example: `pitcheg_vel2sustain_oncc1=50` (Mod wheel increases velocity sensitivity of sustain)
    fn pitcheg_vel2sustain_oncc(&self, cc: i32) -> Result<f32>;
}

impl PitchEnvelopeOpcodes for SfzSection {
    fn pitcheg_attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_attack")
    }
    
    fn pitcheg_decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_decay")
    }
    
    fn pitcheg_delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_delay")
    }
    
    fn pitcheg_hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_hold")
    }
    
    fn pitcheg_release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_release")
    }
    
    fn pitcheg_start(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_start")
    }
    
    fn pitcheg_sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_sustain")
    }
    
    fn pitcheg_depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_depth")
    }
    
    fn pitcheg_vel2attack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2attack")
    }
    
    fn pitcheg_vel2decay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2decay")
    }
    
    fn pitcheg_vel2delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2delay")
    }
    
    fn pitcheg_vel2hold(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2hold")
    }
    
    fn pitcheg_vel2release(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2release")
    }
    
    fn pitcheg_vel2sustain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2sustain")
    }
    
    fn pitcheg_vel2depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2depth")
    }
    
    fn pitcheg_attack_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_attack_shape")
    }
    
    fn pitcheg_decay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_decay_shape")
    }
    
    fn pitcheg_delay_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_delay_shape")
    }
    
    fn pitcheg_hold_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_hold_shape")
    }
    
    fn pitcheg_release_shape(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitcheg_release_shape")
    }
    
    fn pitcheg_attack_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_attack_oncc{}", cc))
    }
    
    fn pitcheg_decay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_decay_oncc{}", cc))
    }
    
    fn pitcheg_delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_delay_oncc{}", cc))
    }
    
    fn pitcheg_hold_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_hold_oncc{}", cc))
    }
    
    fn pitcheg_release_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_release_oncc{}", cc))
    }
    
    fn pitcheg_start_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_start_oncc{}", cc))
    }
    
    fn pitcheg_sustain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_sustain_oncc{}", cc))
    }
    
    fn pitcheg_depth_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_depth_oncc{}", cc))
    }
    
    fn pitcheg_vel2attack_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2attack_oncc{}", cc))
    }
    
    fn pitcheg_vel2decay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2decay_oncc{}", cc))
    }
    
    fn pitcheg_vel2delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2delay_oncc{}", cc))
    }
    
    fn pitcheg_vel2hold_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2hold_oncc{}", cc))
    }
    
    fn pitcheg_vel2release_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2release_oncc{}", cc))
    }
    
    fn pitcheg_vel2sustain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("pitcheg_vel2sustain_oncc{}", cc))
    }
    
    fn pitcheg_vel2curve(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "pitcheg_vel2curve")
    }
} 