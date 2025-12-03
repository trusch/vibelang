use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for filter opcodes
///
/// Filter opcodes control the frequency content of samples by allowing or restricting
/// certain frequencies. They enable the creation of classic synth filter effects,
/// EQ adjustments, and dynamic tonal shaping. SFZ filters can be controlled through
/// envelopes, LFOs, and MIDI controllers, providing extensive sound design capabilities.
pub trait FilterOpcodes {
    /// Gets the filter type
    ///
    /// Specifies which type of filter to use. Common types include:
    /// - lpf_1p/2p/4p/6p: Low-pass filters (1/2/4/6-pole) that allow frequencies below cutoff
    /// - hpf_1p/2p/4p/6p: High-pass filters (1/2/4/6-pole) that allow frequencies above cutoff
    /// - bpf_1p/2p/4p/6p: Band-pass filters that allow frequencies near the cutoff
    /// - brf_1p/2p/4p/6p: Band-reject filters that block frequencies near the cutoff
    /// - pkf_1p/2p/4p/6p: Peak filters that boost frequencies near the cutoff
    /// - lpf_2p_sv: State variable low-pass filter (smoother)
    ///
    /// Example: `fil_type=lpf_2p` (2-pole low-pass filter)
    fn fil_type(&self) -> Result<String>;
    
    /// Gets the filter cutoff frequency in Hz
    ///
    /// Sets the frequency at which the filter begins to take effect.
    /// For low-pass filters, frequencies above this are attenuated.
    /// For high-pass filters, frequencies below this are attenuated.
    ///
    /// Example: `cutoff=1000` (Cutoff frequency at 1000 Hz)
    fn cutoff(&self) -> Result<f32>;
    
    /// Gets the filter cutoff modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the cutoff frequency, in cents.
    /// Allows real-time modulation of the filter cutoff.
    ///
    /// Example: `cutoff_oncc1=2400` (Mod wheel shifts cutoff up to 2400 cents or 2 octaves)
    fn cutoff_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter cutoff smoothing value
    ///
    /// Controls how smooth the transitions are when the cutoff is modulated.
    /// Higher values create more gradual changes to avoid abrupt filter sweeps.
    ///
    /// Example: `cutoff_smoothcc1=50` (Smooth cutoff changes for mod wheel)
    fn cutoff_smoothcc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the cutoff step size for MIDI CC
    ///
    /// Controls the granularity of cutoff changes when modulated by a MIDI CC.
    /// Useful for creating discrete filter steps rather than continuous sweeps.
    ///
    /// Example: `cutoff_stepcc1=200` (Cutoff changes in steps of 200 cents)
    fn cutoff_stepcc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the cutoff curve index for MIDI CC
    ///
    /// Specifies which custom curve to use for mapping CC values to cutoff changes.
    /// References a curve defined elsewhere in the SFZ file.
    ///
    /// Example: `cutoff_curvecca1=3` (Use curve 3 for mapping mod wheel to cutoff)
    fn cutoff_curvecca(&self, cc: i32) -> Result<i32>;
    
    /// Gets the filter cutoff modulation from channel aftertouch
    ///
    /// Controls how much channel aftertouch affects the cutoff frequency, in cents.
    ///
    /// Example: `cutoff_chanaft=1200` (Channel aftertouch shifts cutoff up to 1 octave)
    fn cutoff_chanaft(&self) -> Result<f32>;
    
    /// Gets the filter cutoff modulation from polyphonic aftertouch
    ///
    /// Controls how much polyphonic aftertouch affects the cutoff frequency, in cents.
    ///
    /// Example: `cutoff_polyaft=1200` (Poly aftertouch shifts cutoff up to 1 octave)
    fn cutoff_polyaft(&self) -> Result<f32>;
    
    /// Gets the filter resonance amount
    ///
    /// Controls the emphasis of frequencies near the cutoff point.
    /// Higher values create more pronounced resonant peaks.
    ///
    /// Example: `resonance=10` (High resonance for pronounced filter effect)
    fn resonance(&self) -> Result<f32>;
    
    /// Gets the filter resonance modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the resonance amount.
    ///
    /// Example: `resonance_oncc4=12` (CC 4 increases resonance by up to 12 dB)
    fn resonance_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter resonance smoothing value
    ///
    /// Controls how smooth the transitions are when resonance is modulated.
    /// Higher values create more gradual changes to avoid abrupt resonance changes.
    ///
    /// Example: `resonance_smoothcc4=40` (Smooth resonance changes for CC 4)
    fn resonance_smoothcc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the resonance step size for MIDI CC
    ///
    /// Controls the granularity of resonance changes when modulated by a MIDI CC.
    /// Useful for creating discrete resonance steps rather than continuous changes.
    ///
    /// Example: `resonance_stepcc4=2` (Resonance changes in steps of 2)
    fn resonance_stepcc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the resonance curve index for MIDI CC
    ///
    /// Specifies which custom curve to use for mapping CC values to resonance changes.
    /// References a curve defined elsewhere in the SFZ file.
    ///
    /// Example: `resonance_curvecca4=2` (Use curve 2 for mapping CC 4 to resonance)
    fn resonance_curvecca(&self, cc: i32) -> Result<i32>;
    
    /// Gets the filter keyboard tracking
    ///
    /// Controls how the filter cutoff follows the note being played, in cents per key.
    /// Positive values raise the cutoff for higher notes, creating brighter high notes.
    ///
    /// Example: `fil_keytrack=60` (Cutoff rises 60 cents per octave with note pitch)
    fn fil_keytrack(&self) -> Result<f32>;
    
    /// Gets the keyboard center for filter tracking
    ///
    /// Defines the MIDI note number that serves as the center for fil_keytrack.
    /// No cutoff modification is applied at this key.
    ///
    /// Example: `fil_keycenter=60` (Middle C is the reference key for filter tracking)
    fn fil_keycenter(&self) -> Result<i32>;
    
    /// Gets the filter velocity tracking
    ///
    /// Controls how velocity affects the filter cutoff, in cents.
    /// Positive values raise the cutoff for higher velocities, creating brighter sounds.
    ///
    /// Example: `fil_veltrack=2400` (Filter opens up to 2 octaves with velocity)
    fn fil_veltrack(&self) -> Result<f32>;
    
    /// Gets the random variation for filter cutoff
    ///
    /// Adds a random variation to the cutoff frequency, in cents.
    /// Creates subtle variations between note triggers for more natural sound.
    ///
    /// Example: `fil_random=200` (Random cutoff variation of ±200 cents)
    fn fil_random(&self) -> Result<f32>;
    
    /// Gets the filter LFO depth in cents
    ///
    /// Controls how much the LFO modulates the filter cutoff, in cents.
    /// Used for creating filter wobble, auto-wah, and other modulation effects.
    ///
    /// Example: `fillfo_depth=1200` (LFO modulates the cutoff by ±1 octave)
    fn fillfo_depth(&self) -> Result<f32>;
    
    /// Gets the filter LFO depth modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the filter LFO depth, in cents.
    ///
    /// Example: `fillfo_depth_oncc1=600` (Mod wheel increases LFO depth up to 600 cents)
    fn fillfo_depth_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter LFO frequency in Hz
    ///
    /// Sets the frequency of the filter LFO, in Hertz.
    /// Controls the speed of filter wobble, auto-wah, or other modulations.
    ///
    /// Example: `fillfo_freq=2` (Filter LFO cycles at 2 Hz)
    fn fillfo_freq(&self) -> Result<f32>;
    
    /// Gets the filter LFO frequency modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the filter LFO frequency, in Hertz.
    ///
    /// Example: `fillfo_freq_oncc1=5` (Mod wheel increases LFO speed up to 5 Hz)
    fn fillfo_freq_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter gain in dB
    ///
    /// Controls the output gain of the filter, in decibels.
    /// Compensates for volume changes caused by filter settings.
    ///
    /// Example: `fil_gain=3` (Add 3 dB of gain after filtering)
    fn fil_gain(&self) -> Result<f32>;
    
    /// Gets the filter gain modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the filter gain, in decibels.
    ///
    /// Example: `fil_gain_oncc1=6` (Mod wheel adjusts gain up to 6 dB)
    fn fil_gain_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter Q (alternative for resonance)
    ///
    /// Alternative parameter for controlling filter resonance.
    /// Higher values create more pronounced filter peaks.
    ///
    /// Example: `fil_q=3` (Moderate filter resonance)
    fn fil_q(&self) -> Result<f32>;
    
    /// Gets the filter Q modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the filter Q (resonance).
    ///
    /// Example: `fil_q_oncc4=5` (CC 4 increases Q by up to 5)
    fn fil_q_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the second filter type
    ///
    /// Specifies the type for a second filter in series or parallel.
    /// Same options as fil_type.
    ///
    /// Example: `fil2_type=hpf_2p` (2-pole high-pass filter as second filter)
    fn fil2_type(&self) -> Result<String>;
    
    /// Gets the second filter cutoff frequency in Hz
    ///
    /// Sets the cutoff frequency for the second filter.
    ///
    /// Example: `cutoff2=5000` (Second filter cutoff at 5000 Hz)
    fn cutoff2(&self) -> Result<f32>;
    
    /// Gets the second filter cutoff modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the second filter's cutoff.
    ///
    /// Example: `cutoff2_oncc1=1800` (Mod wheel shifts second cutoff up to 1800 cents)
    fn cutoff2_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the second filter resonance
    ///
    /// Controls the resonance amount for the second filter.
    ///
    /// Example: `resonance2=6` (Moderate resonance for second filter)
    fn resonance2(&self) -> Result<f32>;
    
    /// Gets the second filter resonance modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the second filter's resonance.
    ///
    /// Example: `resonance2_oncc4=8` (CC 4 increases second filter resonance by up to 8)
    fn resonance2_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the filter mode
    ///
    /// Controls how multiple filters are connected:
    /// - serial: First filter feeds into second filter
    /// - parallel: Both filters process the input separately and mix outputs
    ///
    /// Example: `fil_mode=serial` (Filters connected in series)
    fn fil_mode(&self) -> Result<String>;
}

impl FilterOpcodes for SfzSection {
    fn cutoff(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "cutoff")
    }
    
    fn cutoff_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("cutoff_oncc{}", cc))
    }
    
    fn cutoff_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("cutoff_smoothcc{}", cc))
    }
    
    fn cutoff_stepcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("cutoff_stepcc{}", cc))
    }
    
    fn cutoff_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("cutoff_curvecca{}", cc))
    }
    
    fn cutoff_chanaft(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "cutoff_chanaft")
    }
    
    fn cutoff_polyaft(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "cutoff_polyaft")
    }
    
    fn resonance(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "resonance")
    }
    
    fn resonance_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("resonance_oncc{}", cc))
    }
    
    fn resonance_smoothcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("resonance_smoothcc{}", cc))
    }
    
    fn resonance_stepcc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("resonance_stepcc{}", cc))
    }
    
    fn resonance_curvecca(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("resonance_curvecca{}", cc))
    }
    
    fn fil_type(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "fil_type")
    }
    
    fn fil_keytrack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fil_keytrack")
    }
    
    fn fil_keycenter(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "fil_keycenter")
    }
    
    fn fil_veltrack(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fil_veltrack")
    }
    
    fn fil_random(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fil_random")
    }
    
    fn fillfo_depth(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fillfo_depth")
    }
    
    fn fillfo_depth_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fillfo_depth_oncc{}", cc))
    }
    
    fn fillfo_freq(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fillfo_freq")
    }
    
    fn fillfo_freq_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fillfo_freq_oncc{}", cc))
    }
    
    fn fil2_type(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "fil2_type")
    }
    
    fn fil_gain(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fil_gain")
    }
    
    fn fil_gain_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fil_gain_oncc{}", cc))
    }
    
    fn fil_q(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "fil_q")
    }
    
    fn fil_q_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("fil_q_oncc{}", cc))
    }
    
    fn cutoff2(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "cutoff2")
    }
    
    fn cutoff2_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("cutoff2_oncc{}", cc))
    }
    
    fn resonance2(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "resonance2")
    }
    
    fn resonance2_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("resonance2_oncc{}", cc))
    }
    
    fn fil_mode(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "fil_mode")
    }
} 