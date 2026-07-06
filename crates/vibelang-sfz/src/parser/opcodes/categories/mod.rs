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

// All eight opcode-category traits are implemented above. Opcodes outside
// these categories (SFZ v2 extensions, ARIA/sfizz vendor opcodes, MIDI CC
// modulation targets) are accepted by the parser and stored verbatim on
// their section, but have no playback effect; they are surfaced once per
// file via `SfzFile::unknown_opcodes` (see `opcodes::is_known_opcode`).
