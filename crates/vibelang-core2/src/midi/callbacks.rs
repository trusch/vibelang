//! MIDI callback storage and routing builders.
//!
//! This module provides:
//! - Callback storage for MIDI event handlers
//! - Advanced routing builders for keyboards, notes, and CCs

use crate::types::ids::{MidiDeviceId, VoiceId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Type of MIDI callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallbackType {
    /// All MIDI data from a device.
    AllData,
    /// Keyboard note events (note on/off).
    KeyboardNote,
    /// Specific CC number.
    ControlChange(u8),
    /// Pitch bend.
    PitchBend,
    /// MIDI clock sync messages (start, stop, continue, clock pulse).
    ClockSync,
}

/// A stored callback with its metadata.
#[derive(Clone)]
pub struct StoredCallback {
    /// Unique callback ID.
    pub id: u64,
    /// Device this callback is registered for.
    pub device_id: MidiDeviceId,
    /// Type of callback.
    pub callback_type: CallbackType,
    /// Optional channel filter (None = all channels).
    pub channel: Option<u8>,
    /// The callback data (opaque, used by Rhai layer).
    pub callback_data: CallbackData,
}

/// Opaque callback data that can be used by the Rhai layer.
#[derive(Clone)]
pub enum CallbackData {
    /// A placeholder for callbacks that will be handled externally.
    External,
    /// Raw callback data as bytes (for serialization).
    Raw(Vec<u8>),
}

impl std::fmt::Debug for StoredCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCallback")
            .field("id", &self.id)
            .field("device_id", &self.device_id)
            .field("callback_type", &self.callback_type)
            .field("channel", &self.channel)
            .finish()
    }
}

/// Storage for MIDI callbacks.
#[derive(Default)]
pub struct MidiCallbacks {
    /// Next callback ID.
    next_id: AtomicU64,
    /// Callbacks indexed by ID.
    callbacks: HashMap<u64, StoredCallback>,
    /// Index: device -> callback IDs for that device.
    by_device: HashMap<MidiDeviceId, Vec<u64>>,
}

impl MidiCallbacks {
    /// Create new callback storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new callback.
    ///
    /// Returns the unique callback ID.
    pub fn register(
        &mut self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
        channel: Option<u8>,
        callback_data: CallbackData,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let callback = StoredCallback {
            id,
            device_id,
            callback_type,
            channel,
            callback_data,
        };

        self.callbacks.insert(id, callback);
        self.by_device.entry(device_id).or_default().push(id);

        id
    }

    /// Unregister a callback by ID.
    ///
    /// Returns true if the callback was found and removed.
    pub fn unregister(&mut self, id: u64) -> bool {
        if let Some(callback) = self.callbacks.remove(&id) {
            if let Some(ids) = self.by_device.get_mut(&callback.device_id) {
                ids.retain(|&i| i != id);
            }
            true
        } else {
            false
        }
    }

    /// Clear all callbacks.
    pub fn clear(&mut self) {
        self.callbacks.clear();
        self.by_device.clear();
    }

    /// Clear all callbacks for a specific device.
    pub fn clear_device(&mut self, device_id: MidiDeviceId) {
        if let Some(ids) = self.by_device.remove(&device_id) {
            for id in ids {
                self.callbacks.remove(&id);
            }
        }
    }

    /// Get all callbacks for a device.
    pub fn get_for_device(&self, device_id: MidiDeviceId) -> Vec<&StoredCallback> {
        self.by_device
            .get(&device_id)
            .map(|ids| ids.iter().filter_map(|id| self.callbacks.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get callbacks matching a specific type for a device.
    pub fn get_matching(
        &self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
        channel: u8,
    ) -> Vec<&StoredCallback> {
        self.get_for_device(device_id)
            .into_iter()
            .filter(|cb| {
                // Match callback type
                let type_matches = cb.callback_type == callback_type
                    || cb.callback_type == CallbackType::AllData;

                // Match channel filter
                let channel_matches = cb.channel.map_or(true, |ch| ch == channel);

                type_matches && channel_matches
            })
            .collect()
    }

    /// Get callbacks matching a specific type for a device, ignoring channel.
    ///
    /// Used for system messages like clock sync that don't have a channel.
    pub fn get_matching_no_channel(
        &self,
        device_id: MidiDeviceId,
        callback_type: CallbackType,
    ) -> Vec<&StoredCallback> {
        self.get_for_device(device_id)
            .into_iter()
            .filter(|cb| {
                cb.callback_type == callback_type || cb.callback_type == CallbackType::AllData
            })
            .collect()
    }

    /// Get a callback by ID.
    pub fn get(&self, id: u64) -> Option<&StoredCallback> {
        self.callbacks.get(&id)
    }

    /// Get the number of registered callbacks.
    pub fn len(&self) -> usize {
        self.callbacks.len()
    }

    /// Check if there are no callbacks.
    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }
}

// ============================================================================
// Velocity Curves
// ============================================================================

/// Velocity curve types for keyboard routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VelocityCurve {
    /// Linear (default): output = input
    Linear,
    /// Soft: emphasizes low velocities
    Soft,
    /// Hard: emphasizes high velocities
    Hard,
    /// Exponential: dramatic curve
    Exponential,
    /// Compressed: reduces dynamic range
    Compressed,
    /// Fixed: always outputs the same velocity
    Fixed(u8),
}

impl Default for VelocityCurve {
    fn default() -> Self {
        Self::Linear
    }
}

impl VelocityCurve {
    /// Apply the velocity curve to an input velocity (0-127).
    pub fn apply(&self, velocity: u8) -> u8 {
        let v = velocity as f32 / 127.0;
        let result = match self {
            VelocityCurve::Linear => v,
            VelocityCurve::Soft => v.sqrt(),
            VelocityCurve::Hard => v * v,
            VelocityCurve::Exponential => v * v * v,
            VelocityCurve::Compressed => 0.5 + (v - 0.5) * 0.5,
            VelocityCurve::Fixed(fixed) => return *fixed,
        };
        (result * 127.0).clamp(0.0, 127.0) as u8
    }

    /// Parse a velocity curve from a string name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "linear" => Some(Self::Linear),
            "soft" => Some(Self::Soft),
            "hard" => Some(Self::Hard),
            "exponential" | "exp" => Some(Self::Exponential),
            "compressed" | "compress" => Some(Self::Compressed),
            _ => None,
        }
    }
}

// ============================================================================
// Parameter Curves
// ============================================================================

/// Parameter curve types for CC routing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterCurve {
    /// Linear (default)
    Linear,
    /// Logarithmic (good for frequency)
    Logarithmic,
    /// Exponential (good for amplitude)
    Exponential,
}

impl Default for ParameterCurve {
    fn default() -> Self {
        Self::Linear
    }
}

impl ParameterCurve {
    /// Apply the curve to a normalized value (0.0-1.0) and map to range.
    pub fn apply(&self, value: f32, min: f32, max: f32) -> f32 {
        let v = value.clamp(0.0, 1.0);
        let curved = match self {
            ParameterCurve::Linear => v,
            ParameterCurve::Logarithmic => {
                // Map 0-1 to log scale
                let min_log = (min.max(0.001)).ln();
                let max_log = max.ln();
                (min_log + v * (max_log - min_log)).exp()
            }
            ParameterCurve::Exponential => v * v,
        };

        match self {
            ParameterCurve::Logarithmic => curved, // Already in range
            _ => min + curved * (max - min),
        }
    }

    /// Parse a parameter curve from a string name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "linear" => Some(Self::Linear),
            "logarithmic" | "log" => Some(Self::Logarithmic),
            "exponential" | "exp" => Some(Self::Exponential),
            _ => None,
        }
    }
}

// ============================================================================
// Keyboard Route Builder
// ============================================================================

/// Builder for keyboard routing configuration.
#[derive(Debug, Clone)]
pub struct KeyboardRouteBuilder {
    /// Source device.
    pub device_id: MidiDeviceId,
    /// Channel filter (None = all channels).
    pub channel: Option<u8>,
    /// Minimum note (inclusive).
    pub note_min: u8,
    /// Maximum note (inclusive).
    pub note_max: u8,
    /// Transpose in semitones.
    pub transpose: i8,
    /// Velocity curve.
    pub velocity_curve: VelocityCurve,
    /// Target voice.
    pub target_voice: Option<VoiceId>,
}

impl KeyboardRouteBuilder {
    /// Create a new keyboard route builder.
    pub fn new(device_id: MidiDeviceId) -> Self {
        Self {
            device_id,
            channel: None,
            note_min: 0,
            note_max: 127,
            transpose: 0,
            velocity_curve: VelocityCurve::default(),
            target_voice: None,
        }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: u8) -> Self {
        self.channel = Some((channel.saturating_sub(1)) & 0x0F);
        self
    }

    /// Set the note range using MIDI note numbers.
    pub fn range(mut self, min: u8, max: u8) -> Self {
        self.note_min = min.min(127);
        self.note_max = max.min(127);
        self
    }

    /// Set the note range using note names (e.g., "C2", "C6").
    pub fn range_notes(mut self, min: &str, max: &str) -> Self {
        if let Some(min_note) = parse_note_name(min) {
            self.note_min = min_note;
        }
        if let Some(max_note) = parse_note_name(max) {
            self.note_max = max_note;
        }
        self
    }

    /// Transpose by semitones.
    pub fn transpose(mut self, semitones: i8) -> Self {
        self.transpose = semitones;
        self
    }

    /// Transpose by octaves.
    pub fn octave(mut self, octaves: i8) -> Self {
        self.transpose = octaves.saturating_mul(12);
        self
    }

    /// Set the velocity curve.
    pub fn velocity_curve(mut self, curve: VelocityCurve) -> Self {
        self.velocity_curve = curve;
        self
    }

    /// Set the velocity curve by name.
    pub fn velocity_curve_name(mut self, name: &str) -> Self {
        if let Some(curve) = VelocityCurve::from_name(name) {
            self.velocity_curve = curve;
        }
        self
    }

    /// Route to a target voice.
    pub fn to(mut self, voice_id: VoiceId) -> Self {
        self.target_voice = Some(voice_id);
        self
    }

    /// Check if a note is within the configured range.
    pub fn note_in_range(&self, note: u8) -> bool {
        note >= self.note_min && note <= self.note_max
    }

    /// Apply transpose to a note, clamping to valid MIDI range.
    pub fn apply_transpose(&self, note: u8) -> u8 {
        let transposed = note as i16 + self.transpose as i16;
        transposed.clamp(0, 127) as u8
    }

    /// Apply velocity curve.
    pub fn apply_velocity(&self, velocity: u8) -> u8 {
        self.velocity_curve.apply(velocity)
    }
}

// ============================================================================
// Note Route Builder (for drums)
// ============================================================================

/// Builder for single-note routing (e.g., drum pads).
#[derive(Debug, Clone)]
pub struct NoteRouteBuilder {
    /// Source device.
    pub device_id: MidiDeviceId,
    /// Source note number.
    pub source_note: u8,
    /// Channel filter (None = all channels).
    pub channel: Option<u8>,
    /// Choke group name (notes in same group stop each other).
    pub choke_group: Option<String>,
    /// Velocity-to-parameter mapping.
    pub velocity_mapping: Option<VelocityMapping>,
    /// Target voice.
    pub target_voice: Option<VoiceId>,
}

/// Velocity to parameter mapping.
#[derive(Debug, Clone)]
pub struct VelocityMapping {
    /// Parameter name to control.
    pub param: String,
    /// Minimum parameter value (at velocity 0).
    pub min: f32,
    /// Maximum parameter value (at velocity 127).
    pub max: f32,
}

impl NoteRouteBuilder {
    /// Create a new note route builder.
    pub fn new(device_id: MidiDeviceId, note: u8) -> Self {
        Self {
            device_id,
            source_note: note,
            channel: None,
            choke_group: None,
            velocity_mapping: None,
            target_voice: None,
        }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: u8) -> Self {
        self.channel = Some((channel.saturating_sub(1)) & 0x0F);
        self
    }

    /// Set choke group (notes in same group stop each other).
    pub fn choke_group(mut self, group: &str) -> Self {
        self.choke_group = Some(group.to_string());
        self
    }

    /// Map velocity to a parameter.
    pub fn velocity_to(mut self, param: &str, min: f32, max: f32) -> Self {
        self.velocity_mapping = Some(VelocityMapping {
            param: param.to_string(),
            min,
            max,
        });
        self
    }

    /// Route to a target voice.
    pub fn to(mut self, voice_id: VoiceId) -> Self {
        self.target_voice = Some(voice_id);
        self
    }

    /// Calculate parameter value from velocity.
    pub fn velocity_to_param(&self, velocity: u8) -> Option<(String, f32)> {
        self.velocity_mapping.as_ref().map(|mapping| {
            let normalized = velocity as f32 / 127.0;
            let value = mapping.min + normalized * (mapping.max - mapping.min);
            (mapping.param.clone(), value)
        })
    }
}

// ============================================================================
// CC Route Builder
// ============================================================================

/// Builder for CC routing.
#[derive(Debug, Clone)]
pub struct CcRouteBuilder {
    /// Source device.
    pub device_id: MidiDeviceId,
    /// CC number.
    pub cc: u8,
    /// Channel filter (None = all channels).
    pub channel: Option<u8>,
    /// Parameter curve.
    pub curve: ParameterCurve,
    /// Target voice.
    pub target_voice: Option<VoiceId>,
    /// Target parameter name.
    pub target_param: Option<String>,
    /// Parameter range.
    pub range: (f32, f32),
}

impl CcRouteBuilder {
    /// Create a new CC route builder.
    pub fn new(device_id: MidiDeviceId, cc: u8) -> Self {
        Self {
            device_id,
            cc,
            channel: None,
            curve: ParameterCurve::default(),
            target_voice: None,
            target_param: None,
            range: (0.0, 1.0),
        }
    }

    /// Filter to a specific MIDI channel (1-16).
    pub fn channel(mut self, channel: u8) -> Self {
        self.channel = Some((channel.saturating_sub(1)) & 0x0F);
        self
    }

    /// Set the parameter curve.
    pub fn curve(mut self, curve: ParameterCurve) -> Self {
        self.curve = curve;
        self
    }

    /// Set the parameter curve by name.
    pub fn curve_name(mut self, name: &str) -> Self {
        if let Some(curve) = ParameterCurve::from_name(name) {
            self.curve = curve;
        }
        self
    }

    /// Route to a voice parameter with range.
    pub fn to_voice(mut self, voice_id: VoiceId, param: &str, min: f32, max: f32) -> Self {
        self.target_voice = Some(voice_id);
        self.target_param = Some(param.to_string());
        self.range = (min, max);
        self
    }

    /// Calculate parameter value from CC value.
    pub fn cc_to_param(&self, value: u8) -> f32 {
        let normalized = value as f32 / 127.0;
        self.curve.apply(normalized, self.range.0, self.range.1)
    }
}

// ============================================================================
// Utilities
// ============================================================================

/// Parse a note name like "C4" or "F#3" to a MIDI note number.
pub fn parse_note_name(name: &str) -> Option<u8> {
    let name = name.trim().to_uppercase();
    if name.is_empty() {
        return None;
    }

    let mut chars = name.chars().peekable();

    // Get the note letter
    let letter = chars.next()?;
    let base = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };

    // Check for sharp/flat
    let mut modifier = 0i8;
    if let Some(&c) = chars.peek() {
        match c {
            '#' => {
                modifier = 1;
                chars.next();
            }
            'B' => {
                // Could be 'B' note or 'b' for flat
                // Check if followed by a digit
                let temp: String = chars.clone().collect();
                if temp.len() > 1 && temp.chars().nth(1).map_or(false, |c| c.is_ascii_digit()) {
                    modifier = -1;
                    chars.next();
                }
            }
            _ => {}
        }
    }

    // Get the octave
    let octave_str: String = chars.collect();
    let octave: i8 = octave_str.parse().ok()?;

    // MIDI note = (octave + 1) * 12 + base + modifier
    let note = ((octave + 1) as i16) * 12 + base as i16 + modifier as i16;

    if note >= 0 && note <= 127 {
        Some(note as u8)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_curves() {
        // Linear
        assert_eq!(VelocityCurve::Linear.apply(0), 0);
        assert_eq!(VelocityCurve::Linear.apply(127), 127);
        assert_eq!(VelocityCurve::Linear.apply(64), 64);

        // Fixed
        assert_eq!(VelocityCurve::Fixed(100).apply(0), 100);
        assert_eq!(VelocityCurve::Fixed(100).apply(127), 100);

        // Soft (sqrt) should be higher than linear for mid values
        let soft_mid = VelocityCurve::Soft.apply(64);
        assert!(soft_mid > 64);

        // Hard (squared) should be lower than linear for mid values
        let hard_mid = VelocityCurve::Hard.apply(64);
        assert!(hard_mid < 64);
    }

    #[test]
    fn test_parameter_curves() {
        // Linear
        assert_eq!(ParameterCurve::Linear.apply(0.0, 0.0, 100.0), 0.0);
        assert_eq!(ParameterCurve::Linear.apply(1.0, 0.0, 100.0), 100.0);
        assert_eq!(ParameterCurve::Linear.apply(0.5, 0.0, 100.0), 50.0);

        // Exponential at midpoint should be less than linear
        let exp_mid = ParameterCurve::Exponential.apply(0.5, 0.0, 100.0);
        assert!(exp_mid < 50.0);
    }

    #[test]
    fn test_parse_note_name() {
        assert_eq!(parse_note_name("C4"), Some(60));
        assert_eq!(parse_note_name("A4"), Some(69));
        assert_eq!(parse_note_name("C#4"), Some(61));
        assert_eq!(parse_note_name("C0"), Some(12));
        assert_eq!(parse_note_name("C-1"), Some(0));
        assert_eq!(parse_note_name("G9"), Some(127));
    }

    #[test]
    fn test_keyboard_route_builder() {
        let builder = KeyboardRouteBuilder::new(MidiDeviceId::new(0))
            .channel(1)
            .range_notes("C2", "C6")
            .transpose(12)
            .velocity_curve(VelocityCurve::Soft);

        assert_eq!(builder.channel, Some(0)); // 1-indexed to 0-indexed
        assert_eq!(builder.note_min, 36); // C2
        assert_eq!(builder.note_max, 84); // C6
        assert_eq!(builder.transpose, 12);
    }

    #[test]
    fn test_cc_route_builder() {
        let builder = CcRouteBuilder::new(MidiDeviceId::new(0), 74)
            .channel(1)
            .curve(ParameterCurve::Logarithmic)
            .to_voice(VoiceId::new(1), "cutoff", 100.0, 10000.0);

        assert_eq!(builder.cc, 74);
        assert_eq!(builder.channel, Some(0));
        assert_eq!(builder.range, (100.0, 10000.0));
    }

    #[test]
    fn test_midi_callbacks() {
        let mut callbacks = MidiCallbacks::new();

        let id1 = callbacks.register(
            MidiDeviceId::new(0),
            CallbackType::AllData,
            None,
            CallbackData::External,
        );

        let id2 = callbacks.register(
            MidiDeviceId::new(0),
            CallbackType::ControlChange(74),
            Some(0),
            CallbackData::External,
        );

        assert_eq!(callbacks.len(), 2);

        // Get all callbacks for device 0
        let device_callbacks = callbacks.get_for_device(MidiDeviceId::new(0));
        assert_eq!(device_callbacks.len(), 2);

        // Get matching callbacks for CC 74 on channel 0
        let matching = callbacks.get_matching(MidiDeviceId::new(0), CallbackType::ControlChange(74), 0);
        assert_eq!(matching.len(), 2); // Both AllData and the specific CC callback

        // Unregister
        assert!(callbacks.unregister(id1));
        assert_eq!(callbacks.len(), 1);

        // Clear
        callbacks.clear();
        assert!(callbacks.is_empty());
    }
}
