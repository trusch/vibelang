//! MIDI voice output handling.
//!
//! This module provides clean separation of MIDI voice output logic from
//! the regular scsynth voice handling. When a voice has `midi_output` configured,
//! parameter changes are sent as MIDI CC messages instead of (or in addition to)
//! scsynth n_set commands.
//!
//! ## Architecture
//!
//! The module provides pure functions that take all dependencies as parameters,
//! following dependency injection principles. This keeps the code testable and
//! avoids tight coupling with State or MidiHandler.
//!
//! ```text
//! VoicesHandler.set_param()
//!     │
//!     ├── voice.midi_output is None? ──> backend.set_param() (scsynth)
//!     │
//!     └── voice.midi_output is Some? ──> send_cc_for_param()
//!                                            │
//!                                            ├── param not in cc_map? ──> skip
//!                                            │
//!                                            └── convert value ──> send MIDI CC
//! ```

use crate::midi::QueuedMidiEvent;
use crate::traits::VoiceConfig;
use crate::types::MidiDeviceId;
use crossbeam_channel::Sender;
use std::collections::HashMap;

/// Convert a normalized value (0.0-1.0) to MIDI CC value (0-127).
#[inline]
pub fn value_to_cc(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0) as u8
}

/// Send a MIDI CC message for a voice parameter change.
///
/// This is the main entry point for MIDI voice parameter output.
/// Returns true if a MIDI message was sent.
///
/// # Arguments
///
/// * `config` - Voice configuration containing MIDI settings and CC mappings
/// * `param` - Parameter name to send CC for
/// * `value` - Normalized parameter value (typically 0.0-1.0)
/// * `get_sender` - Function to get the MIDI event sender for a device ID
///
/// # Example
///
/// ```ignore
/// // In VoicesHandler.set_param():
/// let sent = send_cc_for_param(
///     &voice.config,
///     param,
///     value,
///     |device_id| output_channels.get(&device_id).cloned()
/// );
/// ```
pub fn send_cc_for_param<F>(
    config: &VoiceConfig,
    param: &str,
    value: f32,
    get_sender: F,
) -> bool
where
    F: FnOnce(MidiDeviceId) -> Option<Sender<QueuedMidiEvent>>,
{
    // Check if voice has MIDI output
    let device_id = match config.midi_output {
        Some(id) => id,
        None => return false,
    };

    // Check if param has CC mapping
    let cc = match config.param_cc_map.get(param) {
        Some(&cc) => cc,
        None => return false,
    };

    // Get MIDI sender
    let sender = match get_sender(device_id) {
        Some(s) => s,
        None => {
            tracing::warn!(
                "MIDI device {:?} not found for voice '{}'",
                device_id,
                config.name
            );
            return false;
        }
    };

    // Convert and send
    let cc_value = value_to_cc(value);
    let event = QueuedMidiEvent::ControlChange {
        channel: config.midi_channel,
        cc,
        value: cc_value,
    };

    if sender.send(event).is_ok() {
        tracing::debug!(
            "MIDI CC: voice='{}' param='{}' cc={} value={} (raw={:.3})",
            config.name,
            param,
            cc,
            cc_value,
            value
        );
        true
    } else {
        tracing::warn!("Failed to send MIDI CC for voice '{}'", config.name);
        false
    }
}

/// Information about a voice's modulator that needs MIDI CC output.
#[derive(Debug, Clone)]
pub struct ModulatorCcMapping {
    /// Parameter name.
    pub param: String,
    /// MIDI CC number.
    pub cc: u8,
    /// Modulator ID (as string for HashMap key).
    pub modulator_id: String,
}

/// Get all modulator-to-CC mappings for a MIDI voice.
///
/// Returns a list of mappings for modulators that have corresponding CC mappings.
/// Used by the runtime tick loop to know which modulator values to poll.
pub fn get_modulator_cc_mappings(config: &VoiceConfig) -> Vec<ModulatorCcMapping> {
    // Only for MIDI voices
    if config.midi_output.is_none() {
        return Vec::new();
    }

    config
        .modulations
        .iter()
        .filter_map(|(param, modulator_id)| {
            // Only include if there's a CC mapping for this param
            config.param_cc_map.get(param).map(|&cc| ModulatorCcMapping {
                param: param.clone(),
                cc,
                modulator_id: modulator_id.0.to_string(),
            })
        })
        .collect()
}

/// Send MIDI CC for modulator values.
///
/// This function takes pre-computed modulator values and sends CC messages
/// for all mapped parameters.
///
/// # Arguments
///
/// * `config` - Voice configuration containing MIDI settings and CC mappings
/// * `modulator_values` - Map from modulator ID string to current value
/// * `sender` - MIDI event sender for this voice's output device
///
/// # Returns
///
/// Number of CC messages sent.
pub fn send_modulator_ccs(
    config: &VoiceConfig,
    modulator_values: &HashMap<String, f32>,
    sender: &Sender<QueuedMidiEvent>,
) -> usize {
    let mut sent = 0;

    for mapping in get_modulator_cc_mappings(config) {
        // Get the modulator's current value
        let value = match modulator_values.get(&mapping.modulator_id) {
            Some(&v) => v,
            None => continue,
        };

        let cc_value = value_to_cc(value);
        let event = QueuedMidiEvent::ControlChange {
            channel: config.midi_channel,
            cc: mapping.cc,
            value: cc_value,
        };

        if sender.send(event).is_ok() {
            tracing::trace!(
                "MIDI mod CC: voice='{}' param='{}' cc={} value={}",
                config.name,
                mapping.param,
                mapping.cc,
                cc_value
            );
            sent += 1;
        }
    }

    sent
}

/// Check if a voice is a MIDI voice (has midi_output configured).
#[inline]
pub fn is_midi_voice(config: &VoiceConfig) -> bool {
    config.midi_output.is_some()
}

/// Check if a voice has any modulator-to-CC mappings.
#[inline]
pub fn has_modulator_cc_mappings(config: &VoiceConfig) -> bool {
    if config.midi_output.is_none() {
        return false;
    }
    config
        .modulations
        .keys()
        .any(|param| config.param_cc_map.contains_key(param))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GroupId, ModulatorId};

    fn make_midi_voice_config() -> VoiceConfig {
        let mut config = VoiceConfig::new("test_voice", "test_synth", GroupId::new(1));
        config.midi_output = Some(MidiDeviceId::new(1));
        config.midi_channel = 0;
        config.param_cc_map.insert("cutoff".to_string(), 74);
        config.param_cc_map.insert("resonance".to_string(), 71);
        config
    }

    #[test]
    fn test_value_to_cc() {
        assert_eq!(value_to_cc(0.0), 0);
        assert_eq!(value_to_cc(1.0), 127);
        assert_eq!(value_to_cc(0.5), 63);
        // Clamping
        assert_eq!(value_to_cc(-0.5), 0);
        assert_eq!(value_to_cc(1.5), 127);
    }

    #[test]
    fn test_send_cc_for_param_no_midi_output() {
        let config = VoiceConfig::new("test", "synth", GroupId::new(1));

        let sent = send_cc_for_param(&config, "cutoff", 0.5, |_| None);
        assert!(!sent);
    }

    #[test]
    fn test_send_cc_for_param_no_cc_mapping() {
        let config = make_midi_voice_config();

        // "freq" is not in cc_map
        let sent = send_cc_for_param(&config, "freq", 0.5, |_| None);
        assert!(!sent);
    }

    #[test]
    fn test_send_cc_for_param_success() {
        let config = make_midi_voice_config();
        let (tx, rx) = crossbeam_channel::unbounded();

        let sent = send_cc_for_param(&config, "cutoff", 0.5, |_| Some(tx.clone()));
        assert!(sent);

        let event = rx.try_recv().unwrap();
        // CC message: 0xB0 + channel, CC number, value
        assert_eq!(event.to_bytes(), vec![0xB0, 74, 63]); // channel 0, CC 74, value 63
    }

    #[test]
    fn test_is_midi_voice() {
        let config = VoiceConfig::new("test", "synth", GroupId::new(1));
        assert!(!is_midi_voice(&config));

        let midi_config = make_midi_voice_config();
        assert!(is_midi_voice(&midi_config));
    }

    #[test]
    fn test_get_modulator_cc_mappings() {
        let mut config = make_midi_voice_config();
        config.modulations.insert("cutoff".to_string(), ModulatorId::new(1));
        config.modulations.insert("resonance".to_string(), ModulatorId::new(2));
        config.modulations.insert("freq".to_string(), ModulatorId::new(3)); // No CC mapping

        let mappings = get_modulator_cc_mappings(&config);
        assert_eq!(mappings.len(), 2); // cutoff and resonance, but not freq
    }

    #[test]
    fn test_has_modulator_cc_mappings() {
        let mut config = make_midi_voice_config();
        assert!(!has_modulator_cc_mappings(&config)); // No modulations yet

        config.modulations.insert("freq".to_string(), ModulatorId::new(1)); // No CC mapping
        assert!(!has_modulator_cc_mappings(&config));

        config.modulations.insert("cutoff".to_string(), ModulatorId::new(2)); // Has CC mapping
        assert!(has_modulator_cc_mappings(&config));
    }
}
