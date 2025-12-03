/// SFZ Opcodes Module
///
/// This module defines the SFZ opcodes and their organization into functional categories.
/// SFZ opcodes are the building blocks of SFZ files, defining how samples should be played.
///
/// # SFZ Opcodes
///
/// Opcodes in SFZ are parameter=value pairs that control various aspects of sample playback.
/// They can be grouped into functional categories such as:
///
/// - **Sound Source Opcodes**: Control which sample to play (`sample`, `default_path`)
/// - **Region Logic Opcodes**: Control when samples play (`key`, `lovel`, `hivel`, `trigger`)
/// - **Performance Opcodes**: Control playback characteristics (`volume`, `pan`, `tune`)
/// - **Envelope Opcodes**: Control amplitude and filter envelopes (`ampeg_attack`, `fileg_decay`)
/// - **Filter Opcodes**: Control filtering of samples (`cutoff`, `resonance`)
/// - **Sample Playback Opcodes**: Control playback details (`loop_mode`, `offset`)
///
/// # Example SFZ file with opcodes
///
/// ```text
/// <control>
/// default_path=samples/piano/
///
/// <global>
/// volume=-6
/// ampeg_release=0.7
///
/// <group>
/// lovel=64
/// hivel=127
///
/// <region>
/// sample=C4.wav
/// key=60
/// ```
///
/// In this example:
/// - `default_path` is a control opcode
/// - `volume` and `ampeg_release` are global opcodes
/// - `lovel` and `hivel` are group opcodes defining a velocity range
/// - `sample` and `key` are region opcodes defining which sample to play and on which key
mod values;
pub mod categories;

pub use self::values::*;
pub use categories::*;

use std::result::Result as StdResult;

use crate::parser::error::Error;
type Result<T> = StdResult<T, Error>;
use crate::parser::types::SfzSection;

/// Trait for type-safe access to SFZ opcodes
///
/// This trait provides methods to access opcode values in a type-safe manner.
/// It allows retrieving opcode values as various types (strings, integers, floats, etc.),
/// with proper error handling for missing or invalid values.
///
/// # SFZ Opcode Access
///
/// In SFZ format, opcodes are simple key=value pairs, but each opcode has an
/// expected type. This trait facilitates retrieving opcodes with the correct type.
pub trait SfzOpcodes {
    /// Get an opcode value as a string
    ///
    /// This method retrieves the raw string value of an opcode.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the opcode to retrieve
    ///
    /// # Returns
    ///
    /// * `Option<&str>` - The opcode value as a string, or None if not found
    fn get_opcode_str(&self, name: &str) -> Option<&str>;
    
    /// Get a typed opcode value
    ///
    /// This method retrieves the value of an opcode and converts it to the specified type.
    /// It uses the `OpcodeValue` trait to handle the conversion.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the opcode to retrieve
    ///
    /// # Returns
    ///
    /// * `Result<T>` - The typed opcode value, or an error if the opcode is missing or invalid
    ///
    /// # Example
    ///
    /// ```
    /// use sfz_parser::{SfzOpcodes, SfzSection, SfzSectionType, opcodes::RegionLogicOpcodes};
    ///
    /// let mut section = SfzSection::new(SfzSectionType::Region);
    /// section.add_opcode("key".to_string(), "60".to_string());
    ///
    /// // Use the trait method to get the key value as an integer
    /// let key = RegionLogicOpcodes::key(&section).unwrap();
    /// assert_eq!(key, 60);
    /// ```
    fn get_opcode<T: OpcodeValue>(&self, name: &str) -> Result<T> {
        match self.get_opcode_str(name) {
            Some(value_str) => OpcodeValue::parse_opcode(value_str),
            None => Err(Error::MissingOpcode(name.to_string())),
        }
    }
}

impl SfzOpcodes for SfzSection {
    fn get_opcode_str(&self, name: &str) -> Option<&str> {
        self.opcodes.get(name).map(|s| s.as_str())
    }
}

/// A struct for extracting information about opcode traits
///
/// This struct provides metadata about the opcode traits, including
/// the trait name and the methods it contains. This is useful for
/// reflective operations on the opcode traits.
///
/// # Opcode Organization
///
/// SFZ opcodes are organized into functional categories, each represented
/// by a trait. This structure allows for exploring these categories and
/// the opcodes they contain.
pub struct OpcodesTraitInfo {
    /// The name of the trait
    pub trait_name: String,
    /// The number of methods (opcodes) in the trait
    pub method_count: usize,
    /// The names of all methods (opcodes) in the trait
    pub methods: Vec<String>,
}

impl OpcodesTraitInfo {
    /// Get information about a trait that implements opcodes
    ///
    /// This method returns metadata about an opcode trait, including
    /// its name and the methods it contains.
    ///
    /// # Type Parameters
    ///
    /// * `T` - The trait to get information about
    ///
    /// # Returns
    ///
    /// * `OpcodesTraitInfo` - Information about the trait
    pub fn for_trait<T: ?Sized>() -> OpcodesTraitInfo {
        let trait_name = std::any::type_name::<T>().to_string();
        let trait_name = trait_name.split("::").last().unwrap_or("Unknown").replace("dyn ", "");
        
        // Count the methods based on the trait name
        let methods = Self::get_methods_for_trait(&trait_name);
        let method_count = methods.len();
        
        OpcodesTraitInfo {
            trait_name,
            method_count,
            methods,
        }
    }
    
    /// Get the method names for a trait
    ///
    /// This method returns all the method names (opcodes) that are part of a
    /// given opcode trait.
    ///
    /// # SFZ Opcode Categories
    ///
    /// SFZ opcodes are organized into several functional categories:
    ///
    /// - **SoundSourceOpcodes**: Control which sample to play and basic source parameters
    /// - **RegionLogicOpcodes**: Control when and how samples are triggered
    /// - **PerformanceOpcodes**: Control volume, panning, tuning, and playback behavior
    /// - **AmplitudeEnvelopeOpcodes**: Control amplitude envelope (ADSR)
    /// - **PitchEnvelopeOpcodes**: Control pitch envelope
    /// - **FilterOpcodes**: Control filter parameters
    /// - **FilterEnvelopeOpcodes**: Control filter envelope
    /// - **SamplePlaybackOpcodes**: Control sample playback details like looping
    ///
    /// # Arguments
    ///
    /// * `trait_name` - The name of the trait to get methods for
    ///
    /// # Returns
    ///
    /// * `Vec<String>` - The names of all methods in the trait
    fn get_methods_for_trait(trait_name: &str) -> Vec<String> {
        match trait_name {
            "SoundSourceOpcodes" => vec![
                // Sound source opcodes
                "sample", "default_path", "sample_quality", "md5", "hash", "phase", "phase_random",
                "oscillator", "oscillator_phase", "oscillator_multi", "oscillator_detune", 
                "oscillator_detune_oncc", "oscillator_mode", "wavetable", "wavetable_position",
                "wavetable_position_oncc", "wavetable_size", "wavetable_mipmaps", "wavetable_multi",
                "wavetable_multi_oncc", "count", "delay_samples", "delay_samples_oncc", "delay",
                "delay_oncc", "delay_random", "direction", "end", "end_oncc", "last_sample",
                "master_delay", "seq_length", "seq_position", "start", "start_oncc", "checksum",
                "wavefile_checksum"
            ].into_iter().map(String::from).collect(),
            
            "RegionLogicOpcodes" => vec![
                // Key mapping
                "lokey", "hikey", "key",
                // Velocity mapping
                "lovel", "hivel",
                // MIDI channel mapping
                "lochan", "hichan",
                // Random RR groups
                "lorand", "hirand",
                // Sequence RR groups
                "seq_length", "seq_position",
                // Trigger conditions
                "trigger", "start_locc", "start_hicc", "stop_locc", "stop_hicc",
                // Key switching
                "sw_lokey", "sw_hikey", "sw_last", "sw_down", "sw_up", "sw_previous", "sw_vel",
                "sw_label", "sw_default",
                // Controller mapping
                "locc", "hicc",
                // Crossfade control
                "xfin_lokey", "xfin_hikey", "xfout_lokey", "xfout_hikey", "xfin_lovel", "xfin_hivel",
                "xfout_lovel", "xfout_hivel", "xf_keycurve", "xf_velcurve", "xf_cccurve",
                // Crossfade for CC
                "xfin_locc", "xfin_hicc", "xfout_locc", "xfout_hicc",
                // MIDI conditions
                "sustain_lo", "sustain_hi", "sostenuto_lo", "sostenuto_hi", "loprog", "hiprog",
                "lobend", "hibend", "lobpm", "hibpm",
                // Aftertouch conditions
                "lochanaft", "hichanaft", "lopolyaft", "hipolyaft"
            ].into_iter().map(String::from).collect(),
            
            "PerformanceOpcodes" => vec![
                // Basic parameters
                "volume", "pan", "width", "position", "amp_veltrack", "amp_velcurve_", "amp_random", 
                "xf_velcurve", "output", "gain_cc", "xfin_gain", "xfout_gain", "amplitude", 
                "amplitude_oncc", "amplitude_smoothcc", "amplitude_curveccN", "global_amplitude", 
                "master_amplitude", "group_amplitude", "note_gain", "note_gain_oncc", "volume_oncc", 
                "gain_oncc", "global_volume", "master_volume", "group_volume", "volume_cc", 
                "volume_smoothcc", "volume_curvecca",
                // Panning
                "pan_cc", "pan_oncc", "pan_smoothcc", "pan_curvecca", "pan_law", "position_oncc", 
                "width_oncc",
                // Pitch
                "transpose", "tune", "pitch_keycenter", "pitch_keytrack", "pitch_veltrack", 
                "pitch_random", "bend_up", "bend_down", "bend_step", "pitch", "pitch_oncc", 
                "pitch_smoothcc", "pitch_curvecca", "bend_smooth", "bend_stepup", "bend_stepdown",
                // Note articulation
                "off_mode", "off_time", "off_shape", "rt_decay", "rt_dead", "sw_vel", "voice_cap", 
                "voice_fader", "voice_fader_oncc", "voice_fader_smoothcc", "voice_fader_curvecca", 
                "polyphony", "polyphony_group", "note_polyphony", "note_selfmask", "sustain_sw",
                // Channel routing
                "delay_cc", "offset_cc", "delay_random_cc", "offset_random_cc",
                // Performance control
                "sustain_cc", "sostenuto_cc", "sostenuto_lo", "sostenuto_hi"
            ].into_iter().map(String::from).collect(),
            
            "AmplitudeEnvelopeOpcodes" => vec![
                // Basic ADSR
                "ampeg_attack", "ampeg_decay", "ampeg_delay", "ampeg_hold",
                "ampeg_release", "ampeg_start", "ampeg_sustain", 
                // Envelope shape
                "ampeg_attack_shape", "ampeg_decay_shape", "ampeg_decay_zero", 
                "ampeg_release_shape", "ampeg_release_zero",
                // Velocity influence
                "ampeg_vel2attack", "ampeg_vel2decay", "ampeg_vel2delay", "ampeg_vel2hold",
                "ampeg_vel2release", "ampeg_vel2sustain",
                // Controller influence
                "ampeg_attackcc", "ampeg_decaycc", "ampeg_delaycc", "ampeg_holdcc",
                "ampeg_releasecc", "ampeg_startcc", "ampeg_sustaincc",
                // Key influence
                "ampeg_attack_oncc", "ampeg_decay_oncc", "ampeg_delay_oncc", "ampeg_hold_oncc",
                "ampeg_release_oncc", "ampeg_start_oncc", "ampeg_sustain_oncc",
                // Dynamic handling
                "ampeg_dynamic", "fileg_dynamic", "pitcheg_dynamic"
            ].into_iter().map(String::from).collect(),
            
            "PitchEnvelopeOpcodes" => vec![
                // Basic ADSR
                "pitcheg_attack", "pitcheg_decay", "pitcheg_delay", "pitcheg_hold",
                "pitcheg_release", "pitcheg_start", "pitcheg_sustain",
                // Envelope depth and shape
                "pitcheg_depth", "pitcheg_attack_shape", "pitcheg_decay_shape", "pitcheg_release_shape",
                "pitcheg_decay_zero", "pitcheg_release_zero",
                // Velocity influence
                "pitcheg_vel2attack", "pitcheg_vel2decay", "pitcheg_vel2delay", "pitcheg_vel2depth",
                "pitcheg_vel2hold", "pitcheg_vel2release", "pitcheg_vel2sustain",
                // CC influence
                "pitcheg_attackcc", "pitcheg_decaycc", "pitcheg_delaycc", "pitcheg_depthcc",
                "pitcheg_holdcc", "pitcheg_releasecc", "pitcheg_startcc", "pitcheg_sustaincc",
                // CC dynamic
                "pitcheg_attack_oncc", "pitcheg_decay_oncc", "pitcheg_delay_oncc", "pitcheg_depth_oncc",
                "pitcheg_hold_oncc", "pitcheg_release_oncc", "pitcheg_start_oncc", "pitcheg_sustain_oncc"
            ].into_iter().map(String::from).collect(),
            
            "FilterOpcodes" => vec![
                // Basic filter parameters
                "cutoff", "cutoff2", "resonance", "resonance2", "fil_type", "fil2_type",
                "fil_keytrack", "fil2_keytrack", "fil_keycenter", "fil2_keycenter",
                "fil_veltrack", "fil2_veltrack", "fil_random", "fil2_random",
                // Filter keyboard tracking
                "cutoff_chanaft", "cutoff_polyaft",
                // CC modulation
                "cutoff_cc", "cutoff2_cc", "cutoff_stepcc", "cutoff2_stepcc",
                "resonance_cc", "resonance2_cc", "cutoff_smoothcc", "cutoff2_smoothcc",
                // MIDI CC dynamic control
                "cutoff_oncc", "cutoff2_oncc", "resonance_oncc", "resonance2_oncc",
                "cutoff_curvecc", "cutoff2_curvecc", "resonance_curvecc", "resonance2_curvecc"
            ].into_iter().map(String::from).collect(),
            
            "FilterEnvelopeOpcodes" => vec![
                // Basic ADSR
                "fileg_attack", "fileg_decay", "fileg_delay", "fileg_hold",
                "fileg_release", "fileg_start", "fileg_sustain",
                // Envelope shapes
                "fileg_attack_shape", "fileg_decay_shape", "fileg_release_shape",
                "fileg_decay_zero", "fileg_release_zero",
                // Envelope depth
                "fileg_depth", "fileg_depth_oncc", "fileg_depthcc",
                // Velocity influence
                "fileg_vel2attack", "fileg_vel2decay", "fileg_vel2delay", "fileg_vel2depth",
                "fileg_vel2hold", "fileg_vel2release", "fileg_vel2sustain",
                // CC influence
                "fileg_attackcc", "fileg_decaycc", "fileg_delaycc", "fileg_holdcc",
                "fileg_releasecc", "fileg_startcc", "fileg_sustaincc",
                // CC dynamic control
                "fileg_attack_oncc", "fileg_decay_oncc", "fileg_delay_oncc", "fileg_hold_oncc",
                "fileg_release_oncc", "fileg_start_oncc", "fileg_sustain_oncc"
            ].into_iter().map(String::from).collect(),
            
            "SamplePlaybackOpcodes" => vec![
                // Loop modes
                "loop_mode", "loop_start", "loop_end", "loop_count",
                // Loop crossfade
                "loop_crossfade", "loop_crossfade_in", "loop_crossfade_out",
                // Sample playback
                "offset", "offset_random", "offset_oncc", "delay", "delay_random", "delay_cc",
                "delay_oncc", "end", "count", "sync_beats", "sync_offset",
                // Direction
                "direction", "waveguide",
                // Sample quality
                "silencer", "note_offset", "tune_oncc", "note_polyphony", "note_selfmask",
                // Sequencing
                "seq_length", "seq_position",
                // Release tricks
                "rt_decay", "rt_dead",
                // Synthetic waveforms
                "oscillator", "oscillator_phase", "oscillator_quality", "oscillator_mode",
                "oscillator_table_size", "oscillator_multi"
            ].into_iter().map(String::from).collect(),
            
            _ => Vec::new(),
        }
    }
} 