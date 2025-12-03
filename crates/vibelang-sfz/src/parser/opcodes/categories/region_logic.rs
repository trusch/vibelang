use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::{TriggerMode, SfzOpcodes};

type Result<T> = std::result::Result<T, Error>;

/// Trait for region logic opcodes that control how regions are selected
///
/// Region logic opcodes determine when a particular region should play based on
/// various conditions including MIDI key ranges, velocity, controllers, and more.
/// These opcodes are essential for creating expressive, dynamic instruments.
pub trait RegionLogicOpcodes {
    /// Gets the lowest key in the key range
    ///
    /// Defines the lower boundary of the key range (MIDI note number 0-127).
    /// If the MIDI note is below this value, the region won't play.
    ///
    /// Example: `lokey=60` (Middle C)
    fn lokey(&self) -> Result<i32>;
    
    /// Gets the highest key in the key range
    ///
    /// Defines the upper boundary of the key range (MIDI note number 0-127).
    /// If the MIDI note is above this value, the region won't play.
    ///
    /// Example: `hikey=72` (C5)
    fn hikey(&self) -> Result<i32>;
    
    /// Gets the key center for the region
    ///
    /// Sets a single key that defines both key range and pitch.
    /// Equivalent to setting `lokey`, `hikey`, and `pitch_keycenter` to the same value.
    /// Useful for one-note samples or drum samples.
    ///
    /// Example: `key=60` (Middle C)
    fn key(&self) -> Result<i32>;
    
    /// Gets the lowest velocity value in the velocity range
    ///
    /// Defines the lower boundary of the velocity range (0-127).
    /// If the MIDI note velocity is below this value, the region won't play.
    ///
    /// Example: `lovel=80` (Medium-high velocity)
    fn lovel(&self) -> Result<i32>;
    
    /// Gets the highest velocity value in the velocity range
    ///
    /// Defines the upper boundary of the velocity range (0-127).
    /// If the MIDI note velocity is above this value, the region won't play.
    ///
    /// Example: `hivel=127` (Maximum velocity)
    fn hivel(&self) -> Result<i32>;
    
    /// Gets the lowest MIDI channel in the channel range
    ///
    /// Defines the lower boundary of the MIDI channel range (1-16).
    /// If the MIDI event is on a channel below this value, the region won't play.
    ///
    /// Example: `lochan=1` (First MIDI channel)
    fn lochan(&self) -> Result<i32>;
    
    /// Gets the highest MIDI channel in the channel range
    ///
    /// Defines the upper boundary of the MIDI channel range (1-16).
    /// If the MIDI event is on a channel above this value, the region won't play.
    ///
    /// Example: `hichan=4` (Fourth MIDI channel)
    fn hichan(&self) -> Result<i32>;
    
    /// Gets the lowest random value
    ///
    /// Defines the lower boundary of a random range (0.0-1.0).
    /// Used for random round-robin sample selection to add variation.
    ///
    /// Example: `lorand=0.0` (Minimum random value)
    fn lorand(&self) -> Result<f32>;
    
    /// Gets the highest random value
    ///
    /// Defines the upper boundary of a random range (0.0-1.0).
    /// Used for random round-robin sample selection to add variation.
    ///
    /// Example: `hirand=0.5` (Half of the random range)
    fn hirand(&self) -> Result<f32>;
    
    /// Gets the sequence length for round-robin groups
    ///
    /// Sets the total number of regions in a round-robin sequence.
    /// Works with `seq_position` to create alternating samples.
    ///
    /// Example: `seq_length=4` (Four-sample sequence)
    fn seq_length(&self) -> Result<i32>;
    
    /// Gets the position in the round-robin sequence
    ///
    /// Specifies which position this region occupies in a round-robin sequence (1-based).
    /// Works with `seq_length` to create alternating samples.
    ///
    /// Example: `seq_position=2` (Second sample in sequence)
    fn seq_position(&self) -> Result<i32>;
    
    /// Gets the trigger mode for the region
    ///
    /// Specifies what event triggers the region to play.
    /// Common modes include: attack (key press), release (key release),
    /// first (only on first note in legato), legato (only on non-first notes in legato).
    ///
    /// Example: `trigger=release` (Plays when key is released)
    fn trigger(&self) -> Result<TriggerMode>;
    
    /// Gets the lowest CC value that starts a region
    ///
    /// Defines the lower boundary of a CC value range that triggers the region.
    /// Used for CC-triggered samples like pedal noises, breath sounds, etc.
    ///
    /// Example: `start_locc64=64` (Sustain pedal halfway down)
    fn start_locc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the highest CC value that starts a region
    ///
    /// Defines the upper boundary of a CC value range that triggers the region.
    /// Used for CC-triggered samples like pedal noises, breath sounds, etc.
    ///
    /// Example: `start_hicc64=127` (Sustain pedal fully down)
    fn start_hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the lowest CC value that stops a region
    ///
    /// Defines the lower boundary of a CC value range that stops the region.
    /// Used to end playback when a controller reaches a certain value.
    ///
    /// Example: `stop_locc64=0` (Sustain pedal fully up)
    fn stop_locc(&self, cc: i32) -> Result<i32>; 
    
    /// Gets the highest CC value that stops a region
    ///
    /// Defines the upper boundary of a CC value range that stops the region.
    /// Used to end playback when a controller reaches a certain value.
    ///
    /// Example: `stop_hicc64=63` (Sustain pedal less than halfway down)
    fn stop_hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the lowest key in the key switch range
    ///
    /// Defines the lower boundary of a key switch range.
    /// Key switches are special keys that select different articulations.
    ///
    /// Example: `sw_lokey=24` (C1)
    fn sw_lokey(&self) -> Result<i32>;
    
    /// Gets the highest key in the key switch range
    ///
    /// Defines the upper boundary of a key switch range.
    /// Key switches are special keys that select different articulations.
    ///
    /// Example: `sw_hikey=36` (C2)
    fn sw_hikey(&self) -> Result<i32>;
    
    /// Gets the last used key switch
    ///
    /// Specifies a key switch that's automatically selected when no other key switch
    /// is active. Useful for setting a default articulation.
    ///
    /// Example: `sw_last=30` (F#1)
    fn sw_last(&self) -> Result<i32>;
    
    /// Gets the down key switch
    ///
    /// Specifies a key switch that's activated when the key is pressed down.
    /// Used for temporary articulation changes.
    ///
    /// Example: `sw_down=28` (E1)
    fn sw_down(&self) -> Result<i32>;
    
    /// Gets the up key switch
    ///
    /// Specifies a key switch that's activated when the key is released.
    /// Used for temporary articulation changes.
    ///
    /// Example: `sw_up=29` (F1)
    fn sw_up(&self) -> Result<i32>;
    
    /// Gets the previous key switch
    ///
    /// Specifies a key switch that returns to the previously used articulation.
    ///
    /// Example: `sw_previous=31` (G1)
    fn sw_previous(&self) -> Result<i32>;
    
    /// Gets the velocity handling mode for key switches
    ///
    /// Specifies how key switch velocities are handled.
    /// Options include: switch (velocity ignored), on (velocity used for selecting articulations).
    ///
    /// Example: `sw_vel=on` (Use velocity for articulation selection)
    fn sw_vel(&self) -> Result<String>;
    
    /// Gets the label for a key switch
    ///
    /// Provides a descriptive label for a key switch articulation.
    /// Useful for display in GUIs.
    ///
    /// Example: `sw_label=Staccato` (Labels this articulation "Staccato")
    fn sw_label(&self) -> Result<String>;
    
    /// Gets the default key switch
    ///
    /// Specifies the default key switch that's active when the instrument loads.
    ///
    /// Example: `sw_default=25` (C#1)
    fn sw_default(&self) -> Result<i32>;

    /// Gets the lowest value of a CC that enables this region
    ///
    /// Defines the lower boundary of a CC value range that enables the region.
    /// If the controller is below this value, the region won't play.
    ///
    /// Example: `locc1=64` (Modulation wheel halfway up)
    fn locc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the highest value of a CC that enables this region
    ///
    /// Defines the upper boundary of a CC value range that enables the region.
    /// If the controller is above this value, the region won't play.
    ///
    /// Example: `hicc1=127` (Modulation wheel fully up)
    fn hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the lowest key for crossfade in
    ///
    /// Defines the lower boundary of a key range where the region fades in.
    /// Used for smooth transitions between regions.
    ///
    /// Example: `xfin_lokey=48` (C3)
    fn xfin_lokey(&self) -> Result<i32>;
    
    /// Gets the highest key for crossfade in
    ///
    /// Defines the upper boundary of a key range where the region fades in.
    /// Used for smooth transitions between regions.
    ///
    /// Example: `xfin_hikey=52` (E3)
    fn xfin_hikey(&self) -> Result<i32>;
    
    /// Gets the lowest key for crossfade out
    ///
    /// Defines the lower boundary of a key range where the region fades out.
    /// Used for smooth transitions between regions.
    ///
    /// Example: `xfout_lokey=53` (F3)
    fn xfout_lokey(&self) -> Result<i32>;
    
    /// Gets the highest key for crossfade out
    ///
    /// Defines the upper boundary of a key range where the region fades out.
    /// Used for smooth transitions between regions.
    ///
    /// Example: `xfout_hikey=60` (C4)
    fn xfout_hikey(&self) -> Result<i32>;
    
    /// Gets the lowest velocity for crossfade in
    ///
    /// Defines the lower boundary of a velocity range where the region fades in.
    /// Used for velocity layering with smooth transitions.
    ///
    /// Example: `xfin_lovel=1` (Lowest velocity)
    fn xfin_lovel(&self) -> Result<i32>;
    
    /// Gets the highest velocity for crossfade in
    ///
    /// Defines the upper boundary of a velocity range where the region fades in.
    /// Used for velocity layering with smooth transitions.
    ///
    /// Example: `xfin_hivel=64` (Half velocity)
    fn xfin_hivel(&self) -> Result<i32>;
    
    /// Gets the lowest velocity for crossfade out
    ///
    /// Defines the lower boundary of a velocity range where the region fades out.
    /// Used for velocity layering with smooth transitions.
    ///
    /// Example: `xfout_lovel=65` (Medium-high velocity)
    fn xfout_lovel(&self) -> Result<i32>;
    
    /// Gets the highest velocity for crossfade out
    ///
    /// Defines the upper boundary of a velocity range where the region fades out.
    /// Used for velocity layering with smooth transitions.
    ///
    /// Example: `xfout_hivel=127` (Highest velocity)
    fn xfout_hivel(&self) -> Result<i32>;
    
    /// Gets the key crossfade curve shape
    ///
    /// Specifies the shape of the crossfade curve for key crossfades.
    /// Common values: power, gain, lin (linear).
    ///
    /// Example: `xf_keycurve=power` (Constant-power curve)
    fn xf_keycurve(&self) -> Result<String>;
    
    /// Gets the velocity crossfade curve shape
    ///
    /// Specifies the shape of the crossfade curve for velocity crossfades.
    /// Common values: power, gain, lin (linear).
    ///
    /// Example: `xf_velcurve=gain` (Constant-gain curve)
    fn xf_velcurve(&self) -> Result<String>;
    
    /// Gets the CC crossfade curve shape
    ///
    /// Specifies the shape of the crossfade curve for CC crossfades.
    /// Common values: power, gain, lin (linear).
    ///
    /// Example: `xf_cccurve=lin` (Linear curve)
    fn xf_cccurve(&self) -> Result<String>;
    
    /// Gets the lowest value of a CC for crossfade in
    ///
    /// Defines the lower boundary of a CC range where the region fades in.
    /// Used for crossfades controlled by MIDI CCs.
    ///
    /// Example: `xfin_locc1=0` (Modulation wheel fully down)
    fn xfin_locc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the highest value of a CC for crossfade in
    ///
    /// Defines the upper boundary of a CC range where the region fades in.
    /// Used for crossfades controlled by MIDI CCs.
    ///
    /// Example: `xfin_hicc1=64` (Modulation wheel halfway up)
    fn xfin_hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the lowest value of a CC for crossfade out
    ///
    /// Defines the lower boundary of a CC range where the region fades out.
    /// Used for crossfades controlled by MIDI CCs.
    ///
    /// Example: `xfout_locc1=65` (Modulation wheel more than halfway up)
    fn xfout_locc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the highest value of a CC for crossfade out
    ///
    /// Defines the upper boundary of a CC range where the region fades out.
    /// Used for crossfades controlled by MIDI CCs.
    ///
    /// Example: `xfout_hicc1=127` (Modulation wheel fully up)
    fn xfout_hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the lowest value of the sustain pedal that enables this region
    ///
    /// Defines the lower boundary of a sustain pedal value range that enables the region.
    /// Used for sustain pedal-specific sounds like pedal noises.
    ///
    /// Example: `sustain_lo=0` (Sustain pedal fully up)
    fn sustain_lo(&self) -> Result<i32>;
    
    /// Gets the highest value of the sustain pedal that enables this region
    ///
    /// Defines the upper boundary of a sustain pedal value range that enables the region.
    /// Used for sustain pedal-specific sounds like pedal noises.
    ///
    /// Example: `sustain_hi=64` (Sustain pedal halfway down)
    fn sustain_hi(&self) -> Result<i32>;
    
    /// Gets the lowest value of the sostenuto pedal that enables this region
    ///
    /// Defines the lower boundary of a sostenuto pedal value range that enables the region.
    /// Used for sostenuto pedal-specific sounds.
    ///
    /// Example: `sostenuto_lo=0` (Sostenuto pedal fully up)
    fn sostenuto_lo(&self) -> Result<i32>;
    
    /// Gets the highest value of the sostenuto pedal that enables this region
    ///
    /// Defines the upper boundary of a sostenuto pedal value range that enables the region.
    /// Used for sostenuto pedal-specific sounds.
    ///
    /// Example: `sostenuto_hi=64` (Sostenuto pedal halfway down)
    fn sostenuto_hi(&self) -> Result<i32>;
    
    /// Gets the lowest MIDI program number that enables this region
    ///
    /// Defines the lower boundary of a MIDI program range that enables the region.
    /// Used for program change-triggered articulations.
    ///
    /// Example: `loprog=0` (First MIDI program)
    fn loprog(&self) -> Result<i32>;
    
    /// Gets the highest MIDI program number that enables this region
    ///
    /// Defines the upper boundary of a MIDI program range that enables the region.
    /// Used for program change-triggered articulations.
    ///
    /// Example: `hiprog=50` (MIDI program 51)
    fn hiprog(&self) -> Result<i32>;
    
    /// Gets the lowest pitch bend value that enables this region
    ///
    /// Defines the lower boundary of a pitch bend range that enables the region.
    /// Range is typically -8192 to 8191.
    ///
    /// Example: `lobend=-8192` (Pitch bend fully down)
    fn lobend(&self) -> Result<i32>;
    
    /// Gets the highest pitch bend value that enables this region
    ///
    /// Defines the upper boundary of a pitch bend range that enables the region.
    /// Range is typically -8192 to 8191.
    ///
    /// Example: `hibend=0` (Pitch bend in center position)
    fn hibend(&self) -> Result<i32>;
    
    /// Gets the lowest tempo in BPM that enables this region
    ///
    /// Defines the lower boundary of a tempo range that enables the region.
    /// Used for tempo-dependent articulations.
    ///
    /// Example: `lobpm=60` (60 BPM)
    fn lobpm(&self) -> Result<f32>;
    
    /// Gets the highest tempo in BPM that enables this region
    ///
    /// Defines the upper boundary of a tempo range that enables the region.
    /// Used for tempo-dependent articulations.
    ///
    /// Example: `hibpm=120` (120 BPM)
    fn hibpm(&self) -> Result<f32>;
    
    /// Gets the lowest channel aftertouch value that enables this region
    ///
    /// Defines the lower boundary of a channel aftertouch range that enables the region.
    ///
    /// Example: `lochanaft=0` (No channel aftertouch)
    fn lochanaft(&self) -> Result<i32>;
    
    /// Gets the highest channel aftertouch value that enables this region
    ///
    /// Defines the upper boundary of a channel aftertouch range that enables the region.
    ///
    /// Example: `hichanaft=64` (Medium channel aftertouch)
    fn hichanaft(&self) -> Result<i32>;
    
    /// Gets the lowest polyphonic aftertouch value that enables this region
    ///
    /// Defines the lower boundary of a polyphonic aftertouch range that enables the region.
    ///
    /// Example: `lopolyaft=0` (No polyphonic aftertouch)
    fn lopolyaft(&self) -> Result<i32>;
    
    /// Gets the highest polyphonic aftertouch value that enables this region
    ///
    /// Defines the upper boundary of a polyphonic aftertouch range that enables the region.
    ///
    /// Example: `hipolyaft=64` (Medium polyphonic aftertouch)
    fn hipolyaft(&self) -> Result<i32>;

    /// A label for a group of regions.
    ///
    /// This provides a user-friendly name for a group of regions, making it easier to identify
    /// different instrument articulations or sections.
    ///
    /// Example: `group_label=Violins Pizzicato`
    fn group_label(&self) -> Result<String>;
}

impl RegionLogicOpcodes for SfzSection {
    // Key mapping
    fn lokey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lokey")
    }
    
    fn hikey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hikey")
    }
    
    fn key(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "key")
    }
    
    // Velocity mapping
    fn lovel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lovel")
    }
    
    fn hivel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hivel")
    }
    
    // MIDI channel mapping
    fn lochan(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lochan")
    }
    
    fn hichan(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hichan")
    }
    
    // Mapping for random RR groups
    fn lorand(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "lorand")
    }
    
    fn hirand(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "hirand")
    }
    
    // Mapping for sequence RR groups
    fn seq_length(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "seq_length")
    }
    
    fn seq_position(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "seq_position")
    }
    
    // Trigger conditions
    fn trigger(&self) -> Result<TriggerMode> {
        SfzOpcodes::get_opcode(self, "trigger")
    }
    
    fn start_locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("start_locc{}", cc))
    }
    
    fn start_hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("start_hicc{}", cc))
    }
    
    fn stop_locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("stop_locc{}", cc))
    }
    
    fn stop_hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("stop_hicc{}", cc))
    }
    
    // Key switching
    fn sw_lokey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_lokey")
    }
    
    fn sw_hikey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_hikey")
    }
    
    fn sw_last(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_last")
    }
    
    fn sw_down(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_down")
    }
    
    fn sw_up(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_up")
    }
    
    fn sw_previous(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_previous")
    }
    
    fn sw_vel(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "sw_vel")
    }
    
    fn sw_label(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "sw_label")
    }
    
    fn sw_default(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sw_default")
    }
    
    // Controller mapping
    fn locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("locc{}", cc))
    }
    
    fn hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("hicc{}", cc))
    }
    
    // Crossfade control
    fn xfin_lokey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfin_lokey")
    }
    
    fn xfin_hikey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfin_hikey")
    }
    
    fn xfout_lokey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfout_lokey")
    }
    
    fn xfout_hikey(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfout_hikey")
    }
    
    fn xfin_lovel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfin_lovel")
    }
    
    fn xfin_hivel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfin_hivel")
    }
    
    fn xfout_lovel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfout_lovel")
    }
    
    fn xfout_hivel(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "xfout_hivel")
    }
    
    fn xf_keycurve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "xf_keycurve")
    }
    
    fn xf_velcurve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "xf_velcurve")
    }
    
    fn xf_cccurve(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "xf_cccurve")
    }
    
    // Crossfade for CC
    fn xfin_locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("xfin_locc{}", cc))
    }
    
    fn xfin_hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("xfin_hicc{}", cc))
    }
    
    fn xfout_locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("xfout_locc{}", cc))
    }
    
    fn xfout_hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("xfout_hicc{}", cc))
    }
    
    // MIDI conditions
    fn sustain_lo(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sustain_lo")
    }
    
    fn sustain_hi(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sustain_hi")
    }
    
    fn sostenuto_lo(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sostenuto_lo")
    }
    
    fn sostenuto_hi(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sostenuto_hi")
    }
    
    fn loprog(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loprog")
    }
    
    fn hiprog(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hiprog")
    }
    
    fn lobend(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lobend")
    }
    
    fn hibend(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hibend")
    }
    
    fn lobpm(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "lobpm")
    }
    
    fn hibpm(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "hibpm")
    }
    
    // Aftertouch conditions
    fn lochanaft(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lochanaft")
    }
    
    fn hichanaft(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hichanaft")
    }
    
    fn lopolyaft(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "lopolyaft")
    }
    
    fn hipolyaft(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "hipolyaft")
    }

    fn group_label(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "group_label")
    }
} 