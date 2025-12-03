mod sound_source;
mod region_logic;
mod performance;
mod amplitude_envelope;
mod pitch_envelope;
mod filter;
mod filter_envelope;
mod sample_playback;

pub use sound_source::SoundSourceOpcodes;
pub use region_logic::RegionLogicOpcodes;
pub use performance::PerformanceOpcodes;
pub use amplitude_envelope::AmplitudeEnvelopeOpcodes;
pub use pitch_envelope::PitchEnvelopeOpcodes;
pub use filter::FilterOpcodes;
pub use filter_envelope::FilterEnvelopeOpcodes;
pub use sample_playback::SamplePlaybackOpcodes;

// TODO: Implement these modules
// mod amplitude_envelope;
// mod pitch_envelope;
// mod filter;
// mod filter_envelope;
// mod sample_playback;
//
// pub use amplitude_envelope::AmplitudeEnvelopeOpcodes;
// pub use pitch_envelope::PitchEnvelopeOpcodes;
// pub use filter::FilterOpcodes;
// pub use filter_envelope::FilterEnvelopeOpcodes;
// pub use sample_playback::SamplePlaybackOpcodes; 