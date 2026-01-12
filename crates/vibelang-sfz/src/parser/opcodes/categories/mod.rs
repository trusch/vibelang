mod amplitude_envelope;
mod filter;
mod filter_envelope;
mod performance;
mod pitch_envelope;
mod region_logic;
mod sample_playback;
mod sound_source;

pub use amplitude_envelope::AmplitudeEnvelopeOpcodes;
pub use filter::FilterOpcodes;
pub use filter_envelope::FilterEnvelopeOpcodes;
pub use performance::PerformanceOpcodes;
pub use pitch_envelope::PitchEnvelopeOpcodes;
pub use region_logic::RegionLogicOpcodes;
pub use sample_playback::SamplePlaybackOpcodes;
pub use sound_source::SoundSourceOpcodes;

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
