//! MIDI Polyphonic Expression (MPE) support.
//!
//! MPE allows each note to have independent pitch bend, pressure, and
//! timbre (CC74) control. This is used by expressive controllers like
//! Roli Seaboard, Linnstrument, Sensel Morph, etc.
//!
//! ## Zone Configuration
//!
//! MPE divides the 16 MIDI channels into zones:
//! - **Lower Zone**: Master on channel 1, members on channels 2-N
//! - **Upper Zone**: Master on channel 16, members on channels 16-N downward
//!
//! Each zone has a "master channel" for global controls and "member channels"
//! for per-note expression.

use std::ops::Range;

// ============================================================================
// MPE Constants
// ============================================================================

/// Default pitch bend range for MPE (semitones).
pub const DEFAULT_PITCH_BEND_RANGE: u8 = 48;

/// CC number for MPE "timbre" / "slide" (Y-axis).
pub const CC_TIMBRE: u8 = 74;

/// CC number for MPE configuration.
/// Standard MIDI constant, reserved for future MPE configuration implementation.
#[allow(dead_code)]
pub const CC_MPE_CONFIGURATION: u8 = 127; // RPN MSB for MPE

/// RPN for pitch bend sensitivity.
/// Standard MIDI constant, reserved for future MPE implementation.
#[allow(dead_code)]
pub const RPN_PITCH_BEND_SENSITIVITY: u16 = 0x0000;

/// RPN for MPE configuration.
/// Standard MIDI constant, reserved for future MPE implementation.
#[allow(dead_code)]
pub const RPN_MPE_CONFIGURATION: u16 = 0x0006;

// ============================================================================
// MPE Zone
// ============================================================================

/// An MPE zone configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpeZone {
    /// Master channel (0 for lower zone, 15 for upper zone).
    pub master_channel: u8,
    /// Member channels for per-note expression.
    pub member_channels: Range<u8>,
    /// Pitch bend range in semitones.
    pub pitch_bend_range: u8,
}

impl MpeZone {
    /// Create a lower zone with the given number of member channels.
    ///
    /// Lower zone uses channel 1 as master, channels 2-N as members.
    pub fn lower(member_count: u8) -> Self {
        let member_count = member_count.min(14); // Max 14 members (channels 2-15)
        Self {
            master_channel: 0,
            member_channels: 1..(1 + member_count),
            pitch_bend_range: DEFAULT_PITCH_BEND_RANGE,
        }
    }

    /// Create an upper zone with the given number of member channels.
    ///
    /// Upper zone uses channel 16 as master, channels 15-N as members.
    pub fn upper(member_count: u8) -> Self {
        let member_count = member_count.min(14);
        let start = 15 - member_count;
        Self {
            master_channel: 15,
            member_channels: start..15,
            pitch_bend_range: DEFAULT_PITCH_BEND_RANGE,
        }
    }

    /// Create a zone with custom pitch bend range.
    pub fn with_pitch_bend_range(mut self, semitones: u8) -> Self {
        self.pitch_bend_range = semitones;
        self
    }

    /// Check if a channel belongs to this zone (master or member).
    pub fn contains_channel(&self, channel: u8) -> bool {
        channel == self.master_channel || self.member_channels.contains(&channel)
    }

    /// Check if a channel is a member channel (not master).
    pub fn is_member_channel(&self, channel: u8) -> bool {
        self.member_channels.contains(&channel)
    }

    /// Check if a channel is the master channel.
    pub fn is_master_channel(&self, channel: u8) -> bool {
        channel == self.master_channel
    }

    /// Get the number of member channels.
    pub fn member_count(&self) -> u8 {
        self.member_channels.len() as u8
    }

    /// Check if this is a lower zone.
    pub fn is_lower(&self) -> bool {
        self.master_channel == 0
    }

    /// Check if this is an upper zone.
    pub fn is_upper(&self) -> bool {
        self.master_channel == 15
    }
}

// ============================================================================
// MPE Configuration
// ============================================================================

/// MPE configuration with optional lower and upper zones.
#[derive(Clone, Debug, Default)]
pub struct MpeConfig {
    /// Lower zone configuration.
    pub lower_zone: Option<MpeZone>,
    /// Upper zone configuration.
    pub upper_zone: Option<MpeZone>,
}

impl MpeConfig {
    /// Create a new empty MPE configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a configuration with only a lower zone.
    pub fn lower_only(member_count: u8) -> Self {
        Self {
            lower_zone: Some(MpeZone::lower(member_count)),
            upper_zone: None,
        }
    }

    /// Create a configuration with only an upper zone.
    pub fn upper_only(member_count: u8) -> Self {
        Self {
            lower_zone: None,
            upper_zone: Some(MpeZone::upper(member_count)),
        }
    }

    /// Create a split configuration with both zones.
    pub fn split(lower_members: u8, upper_members: u8) -> Self {
        Self {
            lower_zone: Some(MpeZone::lower(lower_members)),
            upper_zone: Some(MpeZone::upper(upper_members)),
        }
    }

    /// Check if a channel belongs to any zone.
    pub fn contains_channel(&self, channel: u8) -> bool {
        self.lower_zone
            .as_ref()
            .map(|z| z.contains_channel(channel))
            .unwrap_or(false)
            || self
                .upper_zone
                .as_ref()
                .map(|z| z.contains_channel(channel))
                .unwrap_or(false)
    }

    /// Get the zone for a channel.
    pub fn zone_for_channel(&self, channel: u8) -> Option<&MpeZone> {
        if let Some(ref zone) = self.lower_zone {
            if zone.contains_channel(channel) {
                return Some(zone);
            }
        }
        if let Some(ref zone) = self.upper_zone {
            if zone.contains_channel(channel) {
                return Some(zone);
            }
        }
        None
    }

    /// Check if MPE is enabled (at least one zone configured).
    pub fn is_enabled(&self) -> bool {
        self.lower_zone.is_some() || self.upper_zone.is_some()
    }

    /// Disable MPE (clear all zones).
    pub fn disable(&mut self) {
        self.lower_zone = None;
        self.upper_zone = None;
    }
}

// ============================================================================
// MPE State
// ============================================================================

/// Per-note state for MPE tracking.
#[derive(Clone, Debug, Default)]
pub struct MpeNoteState {
    /// Active note number (if any).
    pub note: Option<u8>,
    /// Current pitch bend value (-8192 to +8191).
    pub pitch_bend: i16,
    /// Current pressure (0-127).
    pub pressure: u8,
    /// Current timbre/slide (CC74, 0-127).
    pub timbre: u8,
}

impl MpeNoteState {
    /// Create a new note state.
    pub fn new(note: u8) -> Self {
        Self {
            note: Some(note),
            pitch_bend: 0,
            pressure: 0,
            timbre: 64, // Center
        }
    }

    /// Clear the note state.
    pub fn clear(&mut self) {
        self.note = None;
        self.pitch_bend = 0;
        self.pressure = 0;
        self.timbre = 64;
    }

    /// Get pitch offset in semitones for a given pitch bend range.
    pub fn pitch_offset(&self, range: u8) -> f32 {
        (self.pitch_bend as f32 / 8192.0) * range as f32
    }

    /// Get normalized pressure (0.0 to 1.0).
    pub fn pressure_normalized(&self) -> f32 {
        self.pressure as f32 / 127.0
    }

    /// Get normalized timbre (0.0 to 1.0).
    pub fn timbre_normalized(&self) -> f32 {
        self.timbre as f32 / 127.0
    }
}

/// Tracks MPE state for all channels.
#[derive(Clone, Debug)]
pub struct MpeState {
    /// Configuration.
    pub config: MpeConfig,
    /// Per-channel note state.
    channel_state: [MpeNoteState; 16],
}

impl MpeState {
    /// Create a new MPE state with the given configuration.
    pub fn new(config: MpeConfig) -> Self {
        Self {
            config,
            channel_state: Default::default(),
        }
    }

    /// Handle a note on event.
    pub fn note_on(&mut self, channel: u8, note: u8) {
        if channel < 16 {
            self.channel_state[channel as usize] = MpeNoteState::new(note);
        }
    }

    /// Handle a note off event.
    pub fn note_off(&mut self, channel: u8) {
        if channel < 16 {
            self.channel_state[channel as usize].clear();
        }
    }

    /// Handle a pitch bend event.
    pub fn pitch_bend(&mut self, channel: u8, value: i16) {
        if channel < 16 {
            self.channel_state[channel as usize].pitch_bend = value;
        }
    }

    /// Handle channel pressure (aftertouch) event.
    pub fn pressure(&mut self, channel: u8, value: u8) {
        if channel < 16 {
            self.channel_state[channel as usize].pressure = value;
        }
    }

    /// Handle timbre (CC74) event.
    pub fn timbre(&mut self, channel: u8, value: u8) {
        if channel < 16 {
            self.channel_state[channel as usize].timbre = value;
        }
    }

    /// Get the state for a channel.
    pub fn get_channel_state(&self, channel: u8) -> &MpeNoteState {
        &self.channel_state[channel.min(15) as usize]
    }

    /// Get mutable state for a channel.
    pub fn get_channel_state_mut(&mut self, channel: u8) -> &mut MpeNoteState {
        &mut self.channel_state[channel.min(15) as usize]
    }

    /// Check if a note is active on any member channel.
    pub fn is_note_active(&self, note: u8) -> bool {
        self.channel_state
            .iter()
            .any(|s| s.note == Some(note))
    }

    /// Get the channel that has a given note active.
    pub fn channel_for_note(&self, note: u8) -> Option<u8> {
        self.channel_state
            .iter()
            .enumerate()
            .find(|(_, s)| s.note == Some(note))
            .map(|(ch, _)| ch as u8)
    }

    /// Get all active notes with their states.
    pub fn active_notes(&self) -> impl Iterator<Item = (u8, &MpeNoteState)> {
        self.channel_state
            .iter()
            .enumerate()
            .filter(|(_, s)| s.note.is_some())
            .map(|(ch, s)| (ch as u8, s))
    }
}

impl Default for MpeState {
    fn default() -> Self {
        Self::new(MpeConfig::default())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lower_zone() {
        let zone = MpeZone::lower(7);
        assert!(zone.is_lower());
        assert!(!zone.is_upper());
        assert_eq!(zone.master_channel, 0);
        assert_eq!(zone.member_channels, 1..8);
        assert!(zone.is_master_channel(0));
        assert!(zone.is_member_channel(1));
        assert!(zone.is_member_channel(7));
        assert!(!zone.is_member_channel(8));
    }

    #[test]
    fn test_upper_zone() {
        let zone = MpeZone::upper(7);
        assert!(!zone.is_lower());
        assert!(zone.is_upper());
        assert_eq!(zone.master_channel, 15);
        assert_eq!(zone.member_channels, 8..15);
    }

    #[test]
    fn test_mpe_config() {
        let config = MpeConfig::split(7, 7);
        assert!(config.is_enabled());
        assert!(config.contains_channel(0)); // Lower master
        assert!(config.contains_channel(5)); // Lower member
        assert!(config.contains_channel(10)); // Upper member
        assert!(config.contains_channel(15)); // Upper master
    }

    #[test]
    fn test_mpe_state() {
        let config = MpeConfig::lower_only(7);
        let mut state = MpeState::new(config);

        state.note_on(3, 60);
        state.pitch_bend(3, 4096);
        state.pressure(3, 80);
        state.timbre(3, 100);

        let ch_state = state.get_channel_state(3);
        assert_eq!(ch_state.note, Some(60));
        assert_eq!(ch_state.pitch_bend, 4096);
        assert_eq!(ch_state.pressure, 80);
        assert_eq!(ch_state.timbre, 100);

        assert!(state.is_note_active(60));
        assert_eq!(state.channel_for_note(60), Some(3));

        state.note_off(3);
        assert!(!state.is_note_active(60));
    }

    #[test]
    fn test_pitch_offset() {
        let mut note = MpeNoteState::new(60);
        note.pitch_bend = 8191; // Max positive

        let offset = note.pitch_offset(48);
        assert!((offset - 48.0).abs() < 0.1);
    }
}
