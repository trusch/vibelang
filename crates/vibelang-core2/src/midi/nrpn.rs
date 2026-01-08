//! NRPN (Non-Registered Parameter Number) and RPN decoder.
//!
//! NRPN/RPN allows 14-bit parameter addressing and values, enabling
//! finer control than standard 7-bit CCs.
//!
//! ## Protocol
//!
//! NRPN/RPN messages are sent as a sequence of CCs:
//! 1. CC 99 (NRPN MSB) or CC 101 (RPN MSB) - Parameter number high byte
//! 2. CC 98 (NRPN LSB) or CC 100 (RPN LSB) - Parameter number low byte
//! 3. CC 6 (Data Entry MSB) - Value high byte
//! 4. CC 38 (Data Entry LSB) - Value low byte (optional)
//!
//! ## Common RPNs
//!
//! - 0x0000: Pitch Bend Sensitivity
//! - 0x0001: Fine Tuning
//! - 0x0002: Coarse Tuning
//! - 0x0006: MPE Configuration (per MIDI 1.0 MPE spec)

// ============================================================================
// CC Numbers
// ============================================================================

/// CC number for Data Entry MSB.
pub const CC_DATA_ENTRY_MSB: u8 = 6;

/// CC number for Data Entry LSB.
pub const CC_DATA_ENTRY_LSB: u8 = 38;

/// CC number for NRPN LSB.
pub const CC_NRPN_LSB: u8 = 98;

/// CC number for NRPN MSB.
pub const CC_NRPN_MSB: u8 = 99;

/// CC number for RPN LSB.
pub const CC_RPN_LSB: u8 = 100;

/// CC number for RPN MSB.
pub const CC_RPN_MSB: u8 = 101;

/// CC number for Data Increment.
pub const CC_DATA_INCREMENT: u8 = 96;

/// CC number for Data Decrement.
pub const CC_DATA_DECREMENT: u8 = 97;

// ============================================================================
// Well-Known RPNs
// ============================================================================

/// RPN for pitch bend sensitivity.
pub const RPN_PITCH_BEND_SENSITIVITY: u16 = 0x0000;

/// RPN for fine tuning.
pub const RPN_FINE_TUNING: u16 = 0x0001;

/// RPN for coarse tuning.
pub const RPN_COARSE_TUNING: u16 = 0x0002;

/// RPN for tuning program select.
pub const RPN_TUNING_PROGRAM: u16 = 0x0003;

/// RPN for tuning bank select.
pub const RPN_TUNING_BANK: u16 = 0x0004;

/// RPN for modulation depth range.
pub const RPN_MOD_DEPTH_RANGE: u16 = 0x0005;

/// RPN for MPE configuration.
pub const RPN_MPE_CONFIGURATION: u16 = 0x0006;

/// NULL RPN (used to cancel selection).
/// Calculated as (0x7F << 7) | 0x7F = 0x3FFF (14-bit value).
pub const RPN_NULL: u16 = 0x3FFF;

// ============================================================================
// Parameter Message
// ============================================================================

/// A decoded NRPN or RPN message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterMessage {
    /// MIDI channel (0-15).
    pub channel: u8,
    /// Whether this is an RPN (true) or NRPN (false).
    pub is_rpn: bool,
    /// Parameter number (0-16383).
    pub parameter: u16,
    /// Parameter value (0-16383).
    pub value: u16,
    /// Whether the value includes the LSB (14-bit) or just MSB (7-bit).
    pub has_lsb: bool,
}

impl ParameterMessage {
    /// Get the 7-bit value (MSB only).
    pub fn value_7bit(&self) -> u8 {
        (self.value >> 7) as u8
    }

    /// Get the full 14-bit value.
    pub fn value_14bit(&self) -> u16 {
        self.value
    }

    /// Get the normalized value (0.0 to 1.0).
    pub fn value_normalized(&self) -> f32 {
        if self.has_lsb {
            self.value as f32 / 16383.0
        } else {
            self.value_7bit() as f32 / 127.0
        }
    }

    /// Check if this is a pitch bend sensitivity message.
    pub fn is_pitch_bend_sensitivity(&self) -> bool {
        self.is_rpn && self.parameter == RPN_PITCH_BEND_SENSITIVITY
    }

    /// Check if this is an MPE configuration message.
    pub fn is_mpe_configuration(&self) -> bool {
        self.is_rpn && self.parameter == RPN_MPE_CONFIGURATION
    }

    /// Check if this is a NULL parameter (cancel selection).
    pub fn is_null(&self) -> bool {
        self.parameter == RPN_NULL
    }
}

// ============================================================================
// Channel State
// ============================================================================

/// State for tracking NRPN/RPN sequences on a single channel.
#[derive(Clone, Debug, Default)]
struct ChannelState {
    /// Selected parameter MSB.
    param_msb: Option<u8>,
    /// Selected parameter LSB.
    param_lsb: Option<u8>,
    /// Whether we're in RPN mode (true) or NRPN mode (false).
    is_rpn: bool,
    /// Data MSB (set before LSB).
    data_msb: Option<u8>,
}

impl ChannelState {
    /// Get the complete parameter number if both MSB and LSB are set.
    fn parameter(&self) -> Option<u16> {
        match (self.param_msb, self.param_lsb) {
            (Some(msb), Some(lsb)) => Some(((msb as u16) << 7) | (lsb as u16)),
            _ => None,
        }
    }

    /// Reset parameter selection.
    fn reset_parameter(&mut self) {
        self.param_msb = None;
        self.param_lsb = None;
        self.data_msb = None;
    }

    /// Reset data entry.
    fn reset_data(&mut self) {
        self.data_msb = None;
    }
}

// ============================================================================
// NRPN/RPN Decoder
// ============================================================================

/// Decoder for NRPN and RPN CC sequences.
///
/// Feed CC messages to `process_cc()` and it will emit complete
/// `ParameterMessage` when a full sequence is received.
pub struct NrpnDecoder {
    /// Per-channel state.
    channel_state: [ChannelState; 16],
}

impl NrpnDecoder {
    /// Create a new decoder.
    pub fn new() -> Self {
        Self {
            channel_state: Default::default(),
        }
    }

    /// Process a CC message and return a parameter message if complete.
    ///
    /// Returns `Some(ParameterMessage)` when a complete NRPN/RPN sequence
    /// has been received, `None` otherwise.
    pub fn process_cc(&mut self, channel: u8, cc: u8, value: u8) -> Option<ParameterMessage> {
        if channel >= 16 {
            return None;
        }

        let state = &mut self.channel_state[channel as usize];

        match cc {
            // RPN parameter select
            CC_RPN_MSB => {
                state.param_msb = Some(value);
                state.is_rpn = true;
                state.reset_data();
                None
            }
            CC_RPN_LSB => {
                state.param_lsb = Some(value);
                state.is_rpn = true;
                state.reset_data();
                None
            }

            // NRPN parameter select
            CC_NRPN_MSB => {
                state.param_msb = Some(value);
                state.is_rpn = false;
                state.reset_data();
                None
            }
            CC_NRPN_LSB => {
                state.param_lsb = Some(value);
                state.is_rpn = false;
                state.reset_data();
                None
            }

            // Data entry
            CC_DATA_ENTRY_MSB => {
                state.data_msb = Some(value);

                // Return 7-bit value if we have a parameter
                if let Some(param) = state.parameter() {
                    Some(ParameterMessage {
                        channel,
                        is_rpn: state.is_rpn,
                        parameter: param,
                        value: (value as u16) << 7,
                        has_lsb: false,
                    })
                } else {
                    None
                }
            }
            CC_DATA_ENTRY_LSB => {
                // Combine with MSB if available
                if let (Some(msb), Some(param)) = (state.data_msb, state.parameter()) {
                    let full_value = ((msb as u16) << 7) | (value as u16);
                    Some(ParameterMessage {
                        channel,
                        is_rpn: state.is_rpn,
                        parameter: param,
                        value: full_value,
                        has_lsb: true,
                    })
                } else {
                    None
                }
            }

            // Data increment/decrement
            CC_DATA_INCREMENT => {
                if let Some(param) = state.parameter() {
                    // Increment the current value
                    let current = state.data_msb.unwrap_or(64) as u16;
                    let new_value = current.saturating_add(1).min(127);
                    state.data_msb = Some(new_value as u8);

                    Some(ParameterMessage {
                        channel,
                        is_rpn: state.is_rpn,
                        parameter: param,
                        value: new_value << 7,
                        has_lsb: false,
                    })
                } else {
                    None
                }
            }
            CC_DATA_DECREMENT => {
                if let Some(param) = state.parameter() {
                    let current = state.data_msb.unwrap_or(64) as u16;
                    let new_value = current.saturating_sub(1);
                    state.data_msb = Some(new_value as u8);

                    Some(ParameterMessage {
                        channel,
                        is_rpn: state.is_rpn,
                        parameter: param,
                        value: new_value << 7,
                        has_lsb: false,
                    })
                } else {
                    None
                }
            }

            _ => None,
        }
    }

    /// Check if a CC number is part of the NRPN/RPN protocol.
    pub fn is_nrpn_cc(cc: u8) -> bool {
        matches!(
            cc,
            CC_DATA_ENTRY_MSB
                | CC_DATA_ENTRY_LSB
                | CC_NRPN_LSB
                | CC_NRPN_MSB
                | CC_RPN_LSB
                | CC_RPN_MSB
                | CC_DATA_INCREMENT
                | CC_DATA_DECREMENT
        )
    }

    /// Get the currently selected parameter for a channel.
    pub fn current_parameter(&self, channel: u8) -> Option<(u16, bool)> {
        if channel >= 16 {
            return None;
        }
        let state = &self.channel_state[channel as usize];
        state.parameter().map(|p| (p, state.is_rpn))
    }

    /// Reset state for a channel.
    pub fn reset_channel(&mut self, channel: u8) {
        if channel < 16 {
            self.channel_state[channel as usize] = ChannelState::default();
        }
    }

    /// Reset all state.
    pub fn reset(&mut self) {
        for state in &mut self.channel_state {
            *state = ChannelState::default();
        }
    }
}

impl Default for NrpnDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpn_sequence() {
        let mut decoder = NrpnDecoder::new();

        // Select pitch bend sensitivity (RPN 0x0000)
        assert!(decoder.process_cc(0, CC_RPN_MSB, 0).is_none());
        assert!(decoder.process_cc(0, CC_RPN_LSB, 0).is_none());

        // Set value to 12 semitones
        let msg = decoder.process_cc(0, CC_DATA_ENTRY_MSB, 12).unwrap();

        assert!(msg.is_rpn);
        assert_eq!(msg.parameter, RPN_PITCH_BEND_SENSITIVITY);
        assert_eq!(msg.value_7bit(), 12);
        assert!(msg.is_pitch_bend_sensitivity());
    }

    #[test]
    fn test_nrpn_sequence() {
        let mut decoder = NrpnDecoder::new();

        // Select NRPN 0x0123
        assert!(decoder.process_cc(0, CC_NRPN_MSB, 0x02).is_none());
        assert!(decoder.process_cc(0, CC_NRPN_LSB, 0x23).is_none());

        // Set 14-bit value
        let msg1 = decoder.process_cc(0, CC_DATA_ENTRY_MSB, 64).unwrap();
        assert!(!msg1.has_lsb);
        assert_eq!(msg1.value_7bit(), 64);

        let msg2 = decoder.process_cc(0, CC_DATA_ENTRY_LSB, 32).unwrap();
        assert!(msg2.has_lsb);
        assert_eq!(msg2.parameter, (0x02 << 7) | 0x23);
        assert_eq!(msg2.value_14bit(), (64 << 7) | 32);
    }

    #[test]
    fn test_mpe_configuration() {
        let mut decoder = NrpnDecoder::new();

        // MPE configuration is RPN 0x0006
        decoder.process_cc(0, CC_RPN_MSB, 0);
        decoder.process_cc(0, CC_RPN_LSB, 6);

        let msg = decoder.process_cc(0, CC_DATA_ENTRY_MSB, 7).unwrap();

        assert!(msg.is_mpe_configuration());
        assert_eq!(msg.value_7bit(), 7); // 7 member channels
    }

    #[test]
    fn test_data_increment_decrement() {
        let mut decoder = NrpnDecoder::new();

        // Select a parameter
        decoder.process_cc(0, CC_NRPN_MSB, 0);
        decoder.process_cc(0, CC_NRPN_LSB, 0);

        // Set initial value
        decoder.process_cc(0, CC_DATA_ENTRY_MSB, 64);

        // Increment
        let msg = decoder.process_cc(0, CC_DATA_INCREMENT, 0).unwrap();
        assert_eq!(msg.value_7bit(), 65);

        // Decrement
        let msg = decoder.process_cc(0, CC_DATA_DECREMENT, 0).unwrap();
        assert_eq!(msg.value_7bit(), 64);
    }

    #[test]
    fn test_null_rpn() {
        let mut decoder = NrpnDecoder::new();

        decoder.process_cc(0, CC_RPN_MSB, 0x7F);
        decoder.process_cc(0, CC_RPN_LSB, 0x7F);

        let msg = decoder.process_cc(0, CC_DATA_ENTRY_MSB, 0).unwrap();
        assert!(msg.is_null());
    }

    #[test]
    fn test_is_nrpn_cc() {
        assert!(NrpnDecoder::is_nrpn_cc(CC_DATA_ENTRY_MSB));
        assert!(NrpnDecoder::is_nrpn_cc(CC_NRPN_MSB));
        assert!(NrpnDecoder::is_nrpn_cc(CC_RPN_LSB));
        assert!(!NrpnDecoder::is_nrpn_cc(74)); // Timbre
        assert!(!NrpnDecoder::is_nrpn_cc(1)); // Mod wheel
    }
}
