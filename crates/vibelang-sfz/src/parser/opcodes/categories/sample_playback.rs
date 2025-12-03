use crate::parser::error::Error;
use crate::parser::types::SfzSection;
use crate::parser::opcodes::{SfzOpcodes, LoopMode, OffMode, TriggerMode};

type Result<T> = std::result::Result<T, Error>;

/// Trait for sample playback opcodes
///
/// Sample playback opcodes control how audio samples are played back, including 
/// their start points, loop behavior, direction, and playback speed. These opcodes
/// are essential for shaping the temporal characteristics of samples and enabling
/// techniques like looping, one-shot playback, reverse playback, and time-stretching.
/// They provide precise control over sample manipulation beyond basic triggering.
pub trait SamplePlaybackOpcodes {
    /// Gets the offset into the sample in samples
    ///
    /// Specifies how many samples to skip at the beginning of the sample.
    /// Useful for trimming unwanted portions or creating variations from the same sample.
    ///
    /// Example: `offset=2048` (Skip the first 2048 samples when playing)
    fn offset(&self) -> Result<i32>;
    
    /// Gets the offset modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the sample offset.
    /// Allows for dynamic control of the sample start point.
    ///
    /// Example: `offset_oncc64=4096` (Sustain pedal increases offset by up to 4096 samples)
    fn offset_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the random offset variation
    ///
    /// Adds a random variation to the sample offset.
    /// Creates variations between note triggers for more natural sound.
    ///
    /// Example: `offset_random=1000` (Random offset variation of up to 1000 samples)
    fn offset_random(&self) -> Result<i32>;
    
    /// Gets the end position in the sample in samples
    ///
    /// Specifies where to stop playback within the sample, in samples.
    /// Allows for using only a portion of a sample.
    ///
    /// Example: `end=24000` (Stop playback at sample 24000)
    fn end(&self) -> Result<i32>;
    
    /// Gets the count of complete sample playbacks
    ///
    /// Specifies how many times to play the sample before stopping.
    /// Useful for creating repeating effects without using loops.
    ///
    /// Example: `count=3` (Play the sample 3 times consecutively)
    fn count(&self) -> Result<i32>;
    
    /// Gets the delay before playback in seconds
    ///
    /// Specifies how long to wait after note-on before starting playback.
    /// Creates staggered entries or allows for layered timing effects.
    ///
    /// Example: `delay=0.5` (Wait 0.5 seconds before playing the sample)
    fn delay(&self) -> Result<f32>;
    
    /// Gets the delay modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the delay time.
    ///
    /// Example: `delay_oncc1=1.0` (Mod wheel increases delay up to 1 second)
    fn delay_oncc(&self, cc: i32) -> Result<f32>;
    
    /// Gets the delay random variation
    ///
    /// Adds a random variation to the delay time.
    /// Creates timing variations between note triggers for more natural sound.
    ///
    /// Example: `delay_random=0.1` (Random delay variation of up to 0.1 seconds)
    fn delay_random(&self) -> Result<f32>;
    
    /// Gets the sample stop mode
    ///
    /// Specifies what happens when a note is released.
    /// Controls whether the sample continues, stops immediately, or fades out.
    ///
    /// Example: `off_mode=normal` (Use normal release behavior)
    fn off_mode(&self) -> Result<OffMode>;
    
    /// Gets the loop mode
    ///
    /// Controls how the sample loops during playback.
    /// Common values include no_loop, one_shot, loop_continuous, and loop_sustain.
    ///
    /// Example: `loop_mode=loop_sustain` (Loop while key is held, then release)
    fn loop_mode(&self) -> Result<LoopMode>;
    
    /// Gets the loop start position in samples
    ///
    /// Specifies where the loop begins within the sample, in samples.
    /// Defines the start point of the looped region.
    ///
    /// Example: `loop_start=12000` (Start the loop at sample 12000)
    fn loop_start(&self) -> Result<i32>;
    
    /// Gets the loop end position in samples
    ///
    /// Specifies where the loop ends within the sample, in samples.
    /// Defines the end point of the looped region.
    ///
    /// Example: `loop_end=24000` (End the loop at sample 24000)
    fn loop_end(&self) -> Result<i32>;
    
    /// Gets the loop crossfade length in samples
    ///
    /// Specifies how many samples to use for crossfading between loop end and start.
    /// Creates smoother loop transitions by blending the end into the beginning.
    ///
    /// Example: `loop_crossfade=1000` (1000 sample crossfade for smoother loops)
    fn loop_crossfade(&self) -> Result<i32>;
    
    /// Gets the number of times to play the loop
    ///
    /// Specifies how many times to repeat the loop before continuing to the rest of the sample.
    /// Useful for creating controlled repetitions.
    ///
    /// Example: `loop_count=4` (Play the loop 4 times, then continue)
    fn loop_count(&self) -> Result<i32>;
    
    /// Gets the sync beat timing
    ///
    /// Controls how the sample synchronizes with the host tempo.
    /// Specified in beats, used for beat-synced sample playback.
    ///
    /// Example: `sync_beats=4` (Sync to 4 beats)
    fn sync_beats(&self) -> Result<f32>;
    
    /// Gets the sync offset in beats
    ///
    /// Specifies an offset from the sync point, in beats.
    /// Adjusts timing of beat-synced samples.
    ///
    /// Example: `sync_offset=0.5` (Offset sync by half a beat)
    fn sync_offset(&self) -> Result<f32>;
    
    /// Gets the playback direction
    ///
    /// Controls whether the sample is played forward or backward.
    /// 1 for forward (normal), -1 for backward (reverse).
    ///
    /// Example: `direction=-1` (Play the sample in reverse)
    fn direction(&self) -> Result<i32>;
    
    /// Gets the time stretching mode
    ///
    /// Specifies which algorithm to use for time-stretching.
    /// Controls how samples are slowed down or sped up while maintaining pitch.
    ///
    /// Example: `timestretch_mode=elastique` (Use Elastique algorithm for time stretching)
    fn timestretch_mode(&self) -> Result<String>;
    
    /// Gets the time stretching ratio
    ///
    /// Controls the speed of playback as a ratio.
    /// Values below 1 slow down, values above 1 speed up.
    ///
    /// Example: `timestretch_ratio=0.5` (Play at half speed)
    fn timestretch_ratio(&self) -> Result<f32>;
    
    /// Gets the pitch shifting mode
    ///
    /// Specifies which algorithm to use for pitch-shifting.
    /// Controls how samples are pitched up or down while maintaining duration.
    ///
    /// Example: `pitchshift_mode=elastique` (Use Elastique algorithm for pitch shifting)
    fn pitchshift_mode(&self) -> Result<String>;
    
    /// Gets the pitch shift amount in semitones
    ///
    /// Controls how much to shift the pitch, in semitones.
    /// Positive values increase pitch, negative values decrease it.
    ///
    /// Example: `pitchshift_amount=12` (Shift pitch up by one octave)
    fn pitchshift_amount(&self) -> Result<f32>;
    
    /// Gets the phase offset between left and right channels
    ///
    /// Controls the phase offset between stereo channels, in degrees.
    /// Creates stereo width effects or phase-based timbral modifications.
    ///
    /// Example: `phase=180` (Invert phase between channels)
    fn phase(&self) -> Result<f32>;
    
    /// Gets the trigger mode
    ///
    /// Specifies what causes the sample to trigger.
    /// Common values include attack (normal), release (play on note-off), first (first note only).
    ///
    /// Example: `trigger=release` (Play the sample when the note is released)
    fn trigger(&self) -> Result<TriggerMode>;
    
    /// Gets the playback rate in percentage
    ///
    /// Controls the speed of sample playback as a percentage.
    /// 100% is normal speed, 50% is half speed, 200% is double speed.
    ///
    /// Example: `playback_rate=50` (Play at half speed)
    fn playback_rate(&self) -> Result<f32>;
    
    /// Gets the chance of the sample playing
    ///
    /// Specifies a probability (0-100%) that the sample will be played when triggered.
    /// Creates randomized sample playback for variation.
    ///
    /// Example: `rt_chance=80` (80% chance the sample will play when triggered)
    fn rt_chance(&self) -> Result<f32>;
    
    /// Gets the number of loop alternations
    ///
    /// Controls ping-pong style looping by alternating between forward and backward.
    /// 0 is normal loop, positive values alternate direction each cycle.
    ///
    /// Example: `loop_alternate=1` (Alternate loop direction each cycle)
    fn loop_alternate(&self) -> Result<i32>;
    
    /// Gets whether to tune the sample
    ///
    /// Controls whether the sample follows keyboard pitch tracking.
    /// 0 disables pitch tracking, 1 enables it (normal behavior).
    ///
    /// Example: `tune=0` (Disable pitch tracking, play at original speed)
    fn tune(&self) -> Result<i32>;
    
    /// Gets the sample quality mode
    ///
    /// Specifies the interpolation quality for resampling.
    /// Higher values use more CPU but provide better sound quality.
    ///
    /// Example: `sample_quality=2` (Use high quality interpolation)
    fn sample_quality(&self) -> Result<i32>;
    
    /// Gets the transpose amount in semitones
    ///
    /// Controls how much to transpose the sample, in semitones.
    /// Affects playback speed and pitch together (unlike pitchshift_amount).
    ///
    /// Example: `transpose=12` (Play one octave higher and faster)
    fn transpose(&self) -> Result<i32>;
    
    /// Gets whether the sample loops seamlessly
    ///
    /// Indicates if the sample has been prepared for seamless looping.
    /// 1 means yes, 0 means no. Affects how loop points are processed.
    ///
    /// Example: `loopwaves=1` (Sample is prepared for seamless looping)
    fn loopwaves(&self) -> Result<i32>;
    
    /// Gets the sample start modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the sample start point.
    ///
    /// Example: `start_oncc1=2000` (Mod wheel increases start point by up to 2000 samples)
    fn start_oncc(&self, cc: i32) -> Result<i32>;
    
    /// Gets the loop crossfade modulation from MIDI CC
    ///
    /// Controls how much a MIDI CC affects the loop crossfade length.
    ///
    /// Example: `loop_crossfade_oncc1=500` (Mod wheel increases crossfade up to 500 samples)
    fn loop_crossfade_oncc(&self, cc: i32) -> Result<i32>;
}

impl SamplePlaybackOpcodes for SfzSection {
    fn loop_mode(&self) -> Result<LoopMode> {
        SfzOpcodes::get_opcode(self, "loop_mode")
    }
    
    fn loop_start(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loop_start")
    }
    
    fn loop_end(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loop_end")
    }
    
    fn loop_crossfade(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loop_crossfade")
    }
    
    fn loop_count(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loop_count")
    }
    
    fn offset(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "offset")
    }
    
    fn offset_random(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "offset_random")
    }
    
    fn offset_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("offset_oncc{}", cc))
    }
    
    fn direction(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "direction")
    }
    
    fn sync_offset(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "sync_offset")
    }
    
    fn sync_beats(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "sync_beats")
    }
    
    fn end(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "end")
    }
    
    fn count(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "count")
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
    
    fn off_mode(&self) -> Result<OffMode> {
        SfzOpcodes::get_opcode(self, "off_mode")
    }
    
    fn timestretch_mode(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "timestretch_mode")
    }
    
    fn timestretch_ratio(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "timestretch_ratio")
    }
    
    fn pitchshift_mode(&self) -> Result<String> {
        SfzOpcodes::get_opcode(self, "pitchshift_mode")
    }
    
    fn pitchshift_amount(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "pitchshift_amount")
    }
    
    fn phase(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "phase")
    }
    
    fn trigger(&self) -> Result<TriggerMode> {
        SfzOpcodes::get_opcode(self, "trigger")
    }
    
    fn playback_rate(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "playback_rate")
    }
    
    fn rt_chance(&self) -> Result<f32> {
        SfzOpcodes::get_opcode(self, "rt_chance")
    }
    
    fn loop_alternate(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loop_alternate")
    }
    
    fn tune(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "tune")
    }
    
    fn sample_quality(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "sample_quality")
    }
    
    fn transpose(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "transpose")
    }
    
    fn loopwaves(&self) -> Result<i32> {
        SfzOpcodes::get_opcode(self, "loopwaves")
    }
    
    fn start_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("start_oncc{}", cc))
    }
    
    fn loop_crossfade_oncc(&self, cc: i32) -> Result<i32> {
        SfzOpcodes::get_opcode(self, &format!("loop_crossfade_oncc{}", cc))
    }
} 