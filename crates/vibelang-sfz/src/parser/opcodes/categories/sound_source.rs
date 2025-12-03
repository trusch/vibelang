use std::path::PathBuf;

use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::SfzOpcodes;

type Result<T> = std::result::Result<T, Error>;

/// Trait for sound source opcodes
///
/// Sound source opcodes define the basic characteristics of the sample source,
/// including file paths, sample playback parameters, and oscillator settings.
pub trait SoundSourceOpcodes {
    /// Gets the sample file path
    ///
    /// This is the most fundamental opcode in SFZ - it specifies which audio file to play.
    /// The path can be absolute or relative to the SFZ file location.
    /// If relative, it's interpreted in relation to the `default_path` setting.
    ///
    /// Example: `sample=piano_C4.wav`
    fn sample(&self) -> Result<PathBuf>;

    /// Gets the default path for sample files
    ///
    /// Sets a default directory for all sample files. This helps avoid repeating
    /// the same path for each sample. Usually set in the `<control>` section.
    ///
    /// Example: `default_path=samples/piano/`
    fn default_path(&self) -> Result<String>;
    
    /// Gets the sample playback quality setting
    ///
    /// Controls the quality of the sample playback. Higher values produce better
    /// quality at the expense of CPU usage. Range is typically 1-4.
    ///
    /// Example: `sample_quality=3`
    fn sample_quality(&self) -> Result<i32>;
    
    /// Gets the MD5 checksum for sample verification
    ///
    /// Used to verify the integrity of a sample file.
    ///
    /// Example: `md5=3a2d0a5741970e3096810caadcb2f38b`
    fn md5(&self) -> Result<String>;
    
    /// Gets the hash value for sample verification
    ///
    /// A general-purpose hash value for verifying sample integrity.
    ///
    /// Example: `hash=1a2b3c4d5e`
    fn hash(&self) -> Result<String>;
    
    /// Gets the checksum for sample verification
    ///
    /// A custom checksum value for the sample.
    ///
    /// Example: `checksum=1234567890`
    fn checksum(&self) -> Result<String>;
    
    /// Gets the wave file checksum for verification
    ///
    /// A checksum specifically for wave files.
    ///
    /// Example: `wavefile_checksum=9876543210`
    fn wavefile_checksum(&self) -> Result<String>;
    
    /// Gets the phase offset for sample playback
    ///
    /// Specifies the phase offset in degrees (0-360) for sample playback.
    /// Useful for layered sounds to adjust phase relationships.
    ///
    /// Example: `phase=90`
    fn phase(&self) -> Result<i32>;
    
    /// Gets the random phase variation
    ///
    /// Adds a random variation to the phase, specified in degrees (0-360).
    /// Creates subtle variations between note triggers for more natural sound.
    ///
    /// Example: `phase_random=45`
    fn phase_random(&self) -> Result<i32>;
    
    /// Checks if oscillator mode is enabled
    ///
    /// When `true`, this region generates sound using an oscillator
    /// rather than playing back a sample.
    ///
    /// Example: `oscillator=1`
    fn oscillator(&self) -> Result<bool>;
    
    /// Gets the oscillator initial phase
    ///
    /// Sets the initial phase for the oscillator in degrees (0-360).
    ///
    /// Example: `oscillator_phase=180`
    fn oscillator_phase(&self) -> Result<f32>;
    
    /// Gets the oscillator multi value
    ///
    /// Controls the number of oscillator instances for stacked oscillators.
    /// Higher values create a richer, detuned sound.
    ///
    /// Example: `oscillator_multi=3`
    fn oscillator_multi(&self) -> Result<i32>;
    
    /// Gets the oscillator detune amount
    ///
    /// Sets the detune amount (in cents) for the oscillator.
    /// Creates a thicker sound when multiple oscillators are used.
    ///
    /// Example: `oscillator_detune=5.5`
    fn oscillator_detune(&self) -> Result<f32>;
    
    /// Gets the oscillator detune amount controlled by MIDI CC
    ///
    /// Adjusts the oscillator detune via a MIDI Continuous Controller.
    ///
    /// Example: `oscillator_detune_oncc1=10.5`
    fn oscillator_detune_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the oscillator mode
    ///
    /// Specifies the waveform type for the oscillator.
    /// Common values: sine, square, triangle, saw, saw_down.
    ///
    /// Example: `oscillator_mode=triangle`
    fn oscillator_mode(&self) -> Result<String>;
    
    /// Gets the oscillator quality setting
    ///
    /// Controls the quality of the oscillator. Higher values provide
    /// better quality at the expense of CPU usage.
    ///
    /// Example: `oscillator_quality=2`
    fn oscillator_quality(&self) -> Result<i32>;
    
    /// Gets the oscillator table size
    ///
    /// Sets the size of the oscillator wavetable in samples.
    /// Larger values provide better quality for complex waveforms.
    ///
    /// Example: `oscillator_table_size=1024`
    fn oscillator_table_size(&self) -> Result<i32>;
    
    /// Gets the wavetable file path
    ///
    /// Specifies a custom wavetable file to use for the oscillator.
    ///
    /// Example: `wavetable=custom_wave.wav`
    fn wavetable(&self) -> Result<String>;
    
    /// Gets the wavetable position
    ///
    /// Controls the position within a multi-frame wavetable (0-1).
    /// Used for morphing between different wavetable frames.
    ///
    /// Example: `wavetable_position=0.5`
    fn wavetable_position(&self) -> Result<f32>;
    
    /// Gets the wavetable position controlled by MIDI CC
    ///
    /// Adjusts the wavetable position via a MIDI Continuous Controller.
    ///
    /// Example: `wavetable_position_oncc1=0.75`
    fn wavetable_position_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the wavetable size
    ///
    /// Sets the size of the wavetable in samples.
    ///
    /// Example: `wavetable_size=2048`
    fn wavetable_size(&self) -> Result<i32>;
    
    /// Gets the number of wavetable mipmaps
    ///
    /// Specifies the number of mipmap levels for the wavetable.
    /// Mipmaps help reduce aliasing at higher frequencies.
    ///
    /// Example: `wavetable_mipmaps=4`
    fn wavetable_mipmaps(&self) -> Result<i32>;
    
    /// Gets the wavetable multi value
    ///
    /// Controls the number of wavetable instances for stacked oscillators.
    ///
    /// Example: `wavetable_multi=2`
    fn wavetable_multi(&self) -> Result<i32>;
    
    /// Gets the wavetable multi amount controlled by MIDI CC
    ///
    /// Adjusts the wavetable multi value via a MIDI Continuous Controller.
    ///
    /// Example: `wavetable_multi_oncc1=3.5`
    fn wavetable_multi_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the number of times to play a sample in sequence
    ///
    /// In round-robin sample groups, this defines how many times a sample is played
    /// before advancing to the next one.
    ///
    /// Example: `count=2`
    fn count(&self) -> Result<i32>;
    
    /// Gets the delay in samples before playback starts
    ///
    /// Specifies a delay in samples before the region starts playing.
    ///
    /// Example: `delay_samples=500`
    fn delay_samples(&self) -> Result<i32>;
    
    /// Gets the delay samples controlled by MIDI CC
    ///
    /// Adjusts the delay in samples via a MIDI Continuous Controller.
    ///
    /// Example: `delay_samples_oncc1=250`
    fn delay_samples_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the delay in seconds before playback starts
    ///
    /// Specifies a delay in seconds before the region starts playing.
    ///
    /// Example: `delay=0.25`
    fn delay(&self) -> Result<f32>;
    
    /// Gets the delay time controlled by MIDI CC
    ///
    /// Adjusts the delay time via a MIDI Continuous Controller.
    ///
    /// Example: `delay_oncc1=0.5`
    fn delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the random delay variation
    ///
    /// Adds a random variation to the delay time (in seconds).
    /// Useful for creating more natural, less mechanical sounds.
    ///
    /// Example: `delay_random=0.1`
    fn delay_random(&self) -> Result<f32>;
    
    /// Gets the playback direction
    ///
    /// Specifies the direction of sample playback.
    /// Common values: forward, reverse, alternate.
    ///
    /// Example: `direction=reverse`
    fn direction(&self) -> Result<String>;
    
    /// Gets the end position in samples
    ///
    /// Sets the end position for sample playback in samples.
    /// Can be used to truncate a sample.
    ///
    /// Example: `end=24000`
    fn end(&self) -> Result<i32>;
    
    /// Gets the end position controlled by MIDI CC
    ///
    /// Adjusts the end position via a MIDI Continuous Controller.
    ///
    /// Example: `end_oncc1=12000`
    fn end_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the last sample position for playback
    ///
    /// Specifies the position of the last sample to play (in samples).
    /// Alternative to `end`, works as `end + 1`.
    ///
    /// Example: `last_sample=24001`
    fn last_sample(&self) -> Result<i32>;
    
    /// Gets the master delay time
    ///
    /// Sets a delay time for all regions in a master section.
    ///
    /// Example: `master_delay=0.1`
    fn master_delay(&self) -> Result<f32>;
    
    /// Gets the output routing number
    ///
    /// Specifies which output channel or bus to route this region to.
    /// Useful for multi-output instruments.
    ///
    /// Example: `output=2`
    fn output(&self) -> Result<i32>;
    
    /// Gets the sample start offset in samples
    ///
    /// Sets an offset (in samples) from the start of the sample file.
    /// Useful for skipping silence or attacks, or creating sound variations.
    ///
    /// Example: `sample_offset=1000`
    fn sample_offset(&self) -> Result<i32>;
    
    /// Gets the sample offset controlled by MIDI CC
    ///
    /// Adjusts the sample offset via a MIDI Continuous Controller.
    ///
    /// Example: `sample_offset_oncc1=500`
    fn sample_offset_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the random sample offset variation
    ///
    /// Adds a random variation to the sample offset (in samples).
    /// Creates subtle variations between note triggers.
    ///
    /// Example: `sample_offset_random=250`
    fn sample_offset_random(&self) -> Result<i32>;
    
    /// Gets the sequence length for round-robin groups
    ///
    /// Sets the total number of regions in a round-robin sequence.
    ///
    /// Example: `seq_length=4`
    fn seq_length(&self) -> Result<i32>;
    
    /// Gets the position in the round-robin sequence
    ///
    /// Specifies which position this region occupies in a round-robin sequence.
    ///
    /// Example: `seq_position=2`
    fn seq_position(&self) -> Result<i32>;
    
    /// Gets the start position in samples
    ///
    /// Sets the start position for sample playback in samples.
    ///
    /// Example: `start=500`
    fn start(&self) -> Result<i32>;
    
    /// Gets the start position controlled by MIDI CC
    ///
    /// Adjusts the start position via a MIDI Continuous Controller.
    ///
    /// Example: `start_oncc1=250`
    fn start_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the sync beat timing
    ///
    /// When time-syncing is active, specifies how many beats to play.
    ///
    /// Example: `sync_beats=4.0`
    fn sync_beats(&self) -> Result<f32>;
    
    /// Gets the sync offset in beats
    ///
    /// When time-syncing is active, specifies an offset in beats.
    ///
    /// Example: `sync_offset=0.5`
    fn sync_offset(&self) -> Result<f32>;
    
    /// Gets the sample trigger mode
    ///
    /// Specifies what triggers the sample playback.
    /// Common values: attack, release, first, legato.
    ///
    /// Example: `trigger=release`
    fn trigger(&self) -> Result<String>;
    
    /// Gets the minimum CC value that triggers this region
    ///
    /// For CC-triggered regions, specifies the minimum CC value to trigger playback.
    ///
    /// Example: `on_locc64=64`
    fn on_locc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the maximum CC value that triggers this region
    ///
    /// For CC-triggered regions, specifies the maximum CC value to trigger playback.
    ///
    /// Example: `on_hicc64=127`
    fn on_hicc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the delay in beats before playback starts
    ///
    /// When time-syncing is active, specifies a delay in beats.
    ///
    /// Example: `delay_beats=2.0`
    fn delay_beats(&self) -> Result<f32>;
    
    /// Gets the duration in beats after which the region stops
    ///
    /// When time-syncing is active, specifies a duration in beats after which
    /// the region stops playing.
    ///
    /// Example: `stop_beats=8.0`
    fn stop_beats(&self) -> Result<f32>;
    
    /// Gets the waveguide configuration for physical modeling
    ///
    /// Used in physical modeling synthesis to define waveguide parameters.
    ///
    /// Example: `waveguide=string_1`
    fn waveguide(&self) -> Result<String>;
}

impl SoundSourceOpcodes for SfzSection {
    fn sample(&self) -> Result<PathBuf> {
        SfzOpcodes::get_opcode(self, "sample")
    }

    fn default_path(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "default_path")
    }

    fn sample_quality(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sample_quality")
    }

    fn md5(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "md5")
    }

    fn hash(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "hash")
    }

    fn checksum(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "checksum")
    }

    fn wavefile_checksum(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "wavefile_checksum")
    }

    fn phase(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "phase")
    }

    fn phase_random(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "phase_random")
    }

    fn oscillator(&self) -> Result<bool> {
        SfzOpcodes::get_opcode(self, "oscillator")
    }

    fn oscillator_phase(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "oscillator_phase")
    }

    fn oscillator_multi(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "oscillator_multi")
    }

    fn oscillator_detune(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "oscillator_detune")
    }

    fn oscillator_detune_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("oscillator_detune_oncc{}", cc))
    }

    fn oscillator_mode(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "oscillator_mode")
    }

    fn oscillator_quality(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "oscillator_quality")
    }

    fn oscillator_table_size(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "oscillator_table_size")
    }

    fn wavetable(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "wavetable")
    }

    fn wavetable_position(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "wavetable_position")
    }

    fn wavetable_position_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("wavetable_position_oncc{}", cc))
    }

    fn wavetable_size(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "wavetable_size")
    }

    fn wavetable_mipmaps(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "wavetable_mipmaps")
    }

    fn wavetable_multi(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "wavetable_multi")
    }

    fn wavetable_multi_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("wavetable_multi_oncc{}", cc))
    }

    fn count(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "count")
    }

    fn delay_samples(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "delay_samples")
    }

    fn delay_samples_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("delay_samples_oncc{}", cc))
    }

    fn delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "delay")
    }

    fn delay_oncc(&self, cc: i32) -> Result<f32> {
        SfzOpcodes::get_opcode(self, &format!("delay_oncc{}", cc))
    }

    fn delay_random(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "delay_random")
    }

    fn direction(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "direction")
    }

    fn end(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "end")
    }

    fn end_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("end_oncc{}", cc))
    }

    fn last_sample(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "last_sample")
    }

    fn master_delay(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "master_delay")
    }

    fn output(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "output")
    }

    fn sample_offset(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sample_offset")
    }

    fn sample_offset_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("sample_offset_oncc{}", cc))
    }

    fn sample_offset_random(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sample_offset_random")
    }

    fn seq_length(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "seq_length")
    }

    fn seq_position(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "seq_position")
    }

    fn start(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "start")
    }

    fn start_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("start_oncc{}", cc))
    }

    fn sync_beats(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "sync_beats")
    }

    fn sync_offset(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "sync_offset")
    }

    fn trigger(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "trigger")
    }

    fn on_locc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("on_locc{}", cc))
    }

    fn on_hicc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("on_hicc{}", cc))
    }

    fn delay_beats(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "delay_beats")
    }

    fn stop_beats(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "stop_beats")
    }

    fn waveguide(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "waveguide")
    }
} 