//! Per-note state tracking for MIDI 2.0 and MPE.
//!
//! This module provides unified per-note expression state tracking that works with
//! both MIDI 2.0 per-note controllers and MPE (MIDI Polyphonic Expression).
//!
//! ## MIDI 2.0 Per-Note Controllers
//!
//! MIDI 2.0 allows individual notes to have independent:
//! - Pitch bend (±N semitones per note)
//! - Pressure (per-note aftertouch)
//! - Controllers (assignable and registered)
//!
//! ## MPE Compatibility
//!
//! MPE uses per-channel expression to achieve similar results:
//! - Each note gets its own MIDI channel
//! - Pitch bend and CC74 (timbre) per channel
//!
//! This module unifies both approaches into a single `PerNoteState` type.

use std::collections::HashMap;
use super::events::{ControlValue, GroupChannel};

/// Default pitch bend range for MIDI 2.0 per-note pitch bend.
pub const DEFAULT_PITCH_BEND_RANGE: u8 = 48;

/// Pitch bend center value for MIDI 2.0 (32-bit).
pub const PITCH_BEND_CENTER: u32 = 0x80000000;

/// Per-note state tracking pitch bend, pressure, and controllers.
#[derive(Clone, Debug, Default)]
pub struct PerNoteState {
    /// Pitch bend offset in semitones.
    pub pitch_bend: f32,
    /// Pressure (aftertouch) as normalized value (0.0-1.0).
    pub pressure: f32,
    /// Timbre/brightness as normalized value (0.0-1.0).
    /// Maps to CC74 in MPE, per-note registered controller 0 in MIDI 2.0.
    pub timbre: f32,
    /// Per-note controllers by index, as normalized values (0.0-1.0).
    pub controllers: HashMap<u8, f32>,
    /// Velocity at note-on time (for reference).
    pub velocity: f32,
}

impl PerNoteState {
    /// Create a new per-note state with default values.
    pub fn new() -> Self {
        Self {
            pitch_bend: 0.0,
            pressure: 0.0,
            timbre: 0.5, // Center
            controllers: HashMap::new(),
            velocity: 0.5,
        }
    }

    /// Create a new per-note state with initial velocity.
    pub fn with_velocity(velocity: f32) -> Self {
        Self {
            velocity,
            ..Self::new()
        }
    }

    /// Set pitch bend from MIDI 2.0 32-bit value.
    pub fn set_pitch_bend_midi2(&mut self, value: u32, range: u8) {
        let centered = (value as i64) - (PITCH_BEND_CENTER as i64);
        self.pitch_bend = (centered as f64 / PITCH_BEND_CENTER as f64 * range as f64) as f32;
    }

    /// Set pitch bend from MPE 14-bit value.
    pub fn set_pitch_bend_mpe(&mut self, value: i16, range: u8) {
        self.pitch_bend = (value as f32 / 8192.0) * range as f32;
    }

    /// Set pressure from MIDI 2.0 32-bit value.
    pub fn set_pressure_midi2(&mut self, value: ControlValue) {
        self.pressure = value.as_f32();
    }

    /// Set pressure from MIDI 1.0/MPE 7-bit value.
    pub fn set_pressure_midi1(&mut self, value: u8) {
        self.pressure = value as f32 / 127.0;
    }

    /// Set timbre from MIDI 2.0 32-bit value.
    pub fn set_timbre_midi2(&mut self, value: ControlValue) {
        self.timbre = value.as_f32();
    }

    /// Set timbre from MIDI 1.0/MPE 7-bit CC74 value.
    pub fn set_timbre_midi1(&mut self, value: u8) {
        self.timbre = value as f32 / 127.0;
    }

    /// Set a per-note controller value.
    pub fn set_controller(&mut self, index: u8, value: f32) {
        self.controllers.insert(index, value);
    }

    /// Set a per-note controller from MIDI 2.0 32-bit value.
    pub fn set_controller_midi2(&mut self, index: u8, value: ControlValue) {
        self.controllers.insert(index, value.as_f32());
    }

    /// Get a per-note controller value.
    pub fn get_controller(&self, index: u8) -> Option<f32> {
        self.controllers.get(&index).copied()
    }

    /// Get the frequency multiplier for pitch bend.
    ///
    /// This can be multiplied with the base frequency to apply pitch bend.
    pub fn pitch_multiplier(&self) -> f32 {
        // Semitones to frequency ratio: 2^(semitones/12)
        2.0_f32.powf(self.pitch_bend / 12.0)
    }

    /// Reset to default values (but keep velocity).
    pub fn reset_controllers(&mut self) {
        self.pitch_bend = 0.0;
        self.pressure = 0.0;
        self.timbre = 0.5;
        self.controllers.clear();
    }
}

/// Manages per-note state for all active notes across all group/channels.
#[derive(Clone, Debug)]
pub struct PerNoteStateManager {
    /// Active notes: (group_channel flat index, note) -> state
    notes: HashMap<(u16, u8), PerNoteState>,
    /// Default pitch bend range in semitones.
    pub pitch_bend_range: u8,
}

impl Default for PerNoteStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PerNoteStateManager {
    /// Create a new per-note state manager.
    pub fn new() -> Self {
        Self {
            notes: HashMap::new(),
            pitch_bend_range: DEFAULT_PITCH_BEND_RANGE,
        }
    }

    /// Create with custom pitch bend range.
    pub fn with_pitch_bend_range(range: u8) -> Self {
        Self {
            notes: HashMap::new(),
            pitch_bend_range: range,
        }
    }

    /// Set the pitch bend range.
    pub fn set_pitch_bend_range(&mut self, range: u8) {
        self.pitch_bend_range = range;
    }

    /// Start tracking a note (called on note-on).
    pub fn note_on(&mut self, gc: GroupChannel, note: u8, velocity: f32) {
        let key = (gc.flat_index(), note);
        self.notes.insert(key, PerNoteState::with_velocity(velocity));
    }

    /// Stop tracking a note (called on note-off).
    pub fn note_off(&mut self, gc: GroupChannel, note: u8) -> Option<PerNoteState> {
        let key = (gc.flat_index(), note);
        self.notes.remove(&key)
    }

    /// Check if a note is currently active.
    pub fn is_active(&self, gc: GroupChannel, note: u8) -> bool {
        let key = (gc.flat_index(), note);
        self.notes.contains_key(&key)
    }

    /// Get the state for an active note.
    pub fn get(&self, gc: GroupChannel, note: u8) -> Option<&PerNoteState> {
        let key = (gc.flat_index(), note);
        self.notes.get(&key)
    }

    /// Get mutable state for an active note.
    pub fn get_mut(&mut self, gc: GroupChannel, note: u8) -> Option<&mut PerNoteState> {
        let key = (gc.flat_index(), note);
        self.notes.get_mut(&key)
    }

    /// Update per-note pitch bend (MIDI 2.0).
    pub fn set_pitch_bend(&mut self, gc: GroupChannel, note: u8, value: u32) {
        let range = self.pitch_bend_range;
        if let Some(state) = self.get_mut(gc, note) {
            state.set_pitch_bend_midi2(value, range);
        }
    }

    /// Update per-note pressure (MIDI 2.0).
    pub fn set_pressure(&mut self, gc: GroupChannel, note: u8, value: ControlValue) {
        if let Some(state) = self.get_mut(gc, note) {
            state.set_pressure_midi2(value);
        }
    }

    /// Update per-note controller (MIDI 2.0).
    pub fn set_controller(&mut self, gc: GroupChannel, note: u8, controller: u8, value: ControlValue) {
        if let Some(state) = self.get_mut(gc, note) {
            state.set_controller_midi2(controller, value);
        }
    }

    /// Reset per-note controllers for a note.
    pub fn reset_controllers(&mut self, gc: GroupChannel, note: u8) {
        if let Some(state) = self.get_mut(gc, note) {
            state.reset_controllers();
        }
    }

    /// Get all active notes for a group/channel.
    pub fn active_notes(&self, gc: GroupChannel) -> impl Iterator<Item = (u8, &PerNoteState)> {
        let prefix = gc.flat_index();
        self.notes
            .iter()
            .filter(move |((gc_idx, _), _)| *gc_idx == prefix)
            .map(|((_, note), state)| (*note, state))
    }

    /// Get all active notes across all group/channels.
    pub fn all_active_notes(&self) -> impl Iterator<Item = (GroupChannel, u8, &PerNoteState)> {
        self.notes.iter().map(|((gc_flat, note), state)| {
            (GroupChannel::from_flat_u16(*gc_flat), *note, state)
        })
    }

    /// Get the number of active notes.
    pub fn active_count(&self) -> usize {
        self.notes.len()
    }

    /// Clear all notes (e.g., on all-notes-off).
    pub fn clear(&mut self) {
        self.notes.clear();
    }

    /// Clear notes for a specific group/channel.
    pub fn clear_group_channel(&mut self, gc: GroupChannel) {
        let prefix = gc.flat_index();
        self.notes.retain(|(gc_idx, _), _| *gc_idx != prefix);
    }
}

/// Per-note route configuration for Rhai API.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct PerNoteRoute {
    /// Controller type (or special value for pitch bend).
    pub controller_type: PerNoteControllerType,
    /// Target voice name.
    pub voice_name: String,
    /// Target parameter name.
    pub param_name: String,
    /// Minimum output value.
    pub min_value: f32,
    /// Maximum output value.
    pub max_value: f32,
    /// Optional curve type.
    pub curve: ParameterCurve,
}

/// Type of per-note controller to route.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PerNoteControllerType {
    /// Per-note pitch bend.
    PitchBend,
    /// Per-note pressure.
    Pressure,
    /// Per-note timbre (slide/brightness).
    Timbre,
    /// Per-note controller by index.
    Controller(u8),
    /// Registered per-note controller.
    RegisteredController(u8),
    /// Assignable per-note controller.
    AssignableController(u8),
}

/// Curve type for parameter mapping.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub enum ParameterCurve {
    #[default]
    Linear,
    Logarithmic,
    Exponential,
    SCurve,
}

#[allow(dead_code)]
impl ParameterCurve {
    /// Apply curve to a normalized value (0.0-1.0).
    pub fn apply(&self, value: f32) -> f32 {
        let v = value.clamp(0.0, 1.0);
        match self {
            ParameterCurve::Linear => v,
            ParameterCurve::Logarithmic => {
                // Fast logarithmic curve: emphasizes lower values
                v.sqrt()
            }
            ParameterCurve::Exponential => {
                // Exponential curve: emphasizes higher values
                v * v
            }
            ParameterCurve::SCurve => {
                // S-curve: smooth transition, emphasizes extremes
                let t = v * 2.0 - 1.0;
                (t * t * t + 1.0) / 2.0
            }
        }
    }
}

#[allow(dead_code)]
impl PerNoteRoute {
    /// Create a new per-note route.
    pub fn new(
        controller_type: PerNoteControllerType,
        voice_name: String,
        param_name: String,
        min_value: f32,
        max_value: f32,
    ) -> Self {
        Self {
            controller_type,
            voice_name,
            param_name,
            min_value,
            max_value,
            curve: ParameterCurve::default(),
        }
    }

    /// Set the curve type.
    pub fn with_curve(mut self, curve: ParameterCurve) -> Self {
        self.curve = curve;
        self
    }

    /// Map a normalized value (0.0-1.0) to the output range.
    pub fn map_value(&self, normalized: f32) -> f32 {
        let curved = self.curve.apply(normalized);
        self.min_value + curved * (self.max_value - self.min_value)
    }

    /// Extract the value from a per-note state for this route.
    pub fn get_value(&self, state: &PerNoteState) -> f32 {
        match &self.controller_type {
            PerNoteControllerType::PitchBend => {
                // Pitch bend is in semitones, not normalized
                // Return the raw semitones value for direct use
                state.pitch_bend
            }
            PerNoteControllerType::Pressure => self.map_value(state.pressure),
            PerNoteControllerType::Timbre => self.map_value(state.timbre),
            PerNoteControllerType::Controller(idx) => {
                let v = state.get_controller(*idx).unwrap_or(0.5);
                self.map_value(v)
            }
            PerNoteControllerType::RegisteredController(idx) => {
                let v = state.get_controller(*idx).unwrap_or(0.5);
                self.map_value(v)
            }
            PerNoteControllerType::AssignableController(idx) => {
                let v = state.get_controller(*idx).unwrap_or(0.5);
                self.map_value(v)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_per_note_state_pitch_bend() {
        let mut state = PerNoteState::new();

        // MIDI 2.0 center = 0 semitones
        state.set_pitch_bend_midi2(0x80000000, 48);
        assert!(state.pitch_bend.abs() < 0.001);

        // MIDI 2.0 max = +48 semitones
        state.set_pitch_bend_midi2(0xFFFFFFFF, 48);
        assert!((state.pitch_bend - 48.0).abs() < 0.1);

        // MIDI 2.0 min = -48 semitones
        state.set_pitch_bend_midi2(0, 48);
        assert!((state.pitch_bend + 48.0).abs() < 0.1);
    }

    #[test]
    fn test_per_note_state_pitch_multiplier() {
        let mut state = PerNoteState::new();

        // +12 semitones = 2x frequency (one octave up)
        state.pitch_bend = 12.0;
        assert!((state.pitch_multiplier() - 2.0).abs() < 0.01);

        // -12 semitones = 0.5x frequency (one octave down)
        state.pitch_bend = -12.0;
        assert!((state.pitch_multiplier() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_per_note_state_manager() {
        let mut manager = PerNoteStateManager::new();
        let gc = GroupChannel::new(0, 0);

        // Note on
        manager.note_on(gc, 60, 0.8);
        assert!(manager.is_active(gc, 60));
        assert!(!manager.is_active(gc, 61));

        // Get state
        let state = manager.get(gc, 60).unwrap();
        assert!((state.velocity - 0.8).abs() < 0.001);

        // Set pitch bend
        manager.set_pitch_bend(gc, 60, 0xC0000000); // +0.5 range
        let state = manager.get(gc, 60).unwrap();
        assert!((state.pitch_bend - 24.0).abs() < 0.5); // +24 semitones

        // Note off
        let removed = manager.note_off(gc, 60);
        assert!(removed.is_some());
        assert!(!manager.is_active(gc, 60));
    }

    #[test]
    fn test_parameter_curve() {
        let linear = ParameterCurve::Linear;
        assert!((linear.apply(0.5) - 0.5).abs() < 0.001);

        let log = ParameterCurve::Logarithmic;
        // At 0.25, sqrt(0.25) = 0.5
        assert!((log.apply(0.25) - 0.5).abs() < 0.001);

        let exp = ParameterCurve::Exponential;
        // At 0.5, 0.5^2 = 0.25
        assert!((exp.apply(0.5) - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_per_note_route() {
        let route = PerNoteRoute::new(
            PerNoteControllerType::Pressure,
            "lead".to_string(),
            "filter".to_string(),
            200.0,
            8000.0,
        );

        let mut state = PerNoteState::new();
        state.pressure = 0.5;

        let value = route.get_value(&state);
        // Linear: 200 + 0.5 * (8000 - 200) = 200 + 3900 = 4100
        assert!((value - 4100.0).abs() < 1.0);
    }
}
