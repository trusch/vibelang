//! MIDI routing management.
//!
//! This module handles registration and application of MIDI routes
//! for keyboards, notes, CCs, and MIDI 2.0 features.

use super::types::{CcRoute, KeyboardRoute, MidiRouting};

use super::types::{Midi2ControllerType, Midi2KeyboardRoute, Midi2PerNoteRoute};
use crate::compat::RwLock;
use crate::midi::{
    canonical_cc_curve_name, is_pipewire_midi_input_id, CcRouteBuilder, KeyboardRouteBuilder,
    NoteRouteBuilder, ParameterCurve, VelocityCurve, VelocityMapping,
};
use crate::types::ids::MidiDeviceId;
use crate::types::VoiceId;
use std::sync::Arc;

/// Parse a velocity curve string (as stored in ScriptState) back to VelocityCurve.
fn parse_velocity_curve(s: &str) -> VelocityCurve {
    if let Some(val) = s.strip_prefix("fixed:") {
        if let Ok(v) = val.parse::<u8>() {
            return VelocityCurve::Fixed(v);
        }
    }
    VelocityCurve::from_name(s).unwrap_or_default()
}

fn parse_parameter_curve(s: &str) -> ParameterCurve {
    ParameterCurve::from_name(s).unwrap_or_default()
}

/// Manager for MIDI routing configuration.
pub struct MidiRoutingManager {
    /// Routing configuration.
    routing: Arc<RwLock<MidiRouting>>,
}

impl Default for MidiRoutingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiRoutingManager {
    /// Create a new routing manager.
    pub fn new() -> Self {
        Self {
            routing: Arc::new(RwLock::new(MidiRouting::default())),
        }
    }

    /// Get the routing configuration.
    pub fn routing(&self) -> Arc<RwLock<MidiRouting>> {
        Arc::clone(&self.routing)
    }

    /// Add an advanced keyboard route.
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_keyboard_route(&self, route: KeyboardRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.advanced_keyboard_routes.len();
        tracing::info!(
            "Added keyboard route {} for device {} (notes {}-{}, transpose {})",
            idx,
            route.device_id.0,
            route.note_min,
            route.note_max,
            route.transpose
        );
        routing.advanced_keyboard_routes.push(route);
        idx
    }

    /// Add an advanced note route (for drums/pads).
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_note_route(&self, route: NoteRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.note_routes.len();
        tracing::info!(
            "Added note route {} for device {} note {}",
            idx,
            route.device_id.0,
            route.source_note
        );
        routing.note_routes.push(route);
        idx
    }

    /// Add an advanced CC route.
    ///
    /// Returns the index of the route for later removal.
    pub async fn add_cc_route(&self, route: CcRouteBuilder) -> usize {
        let mut routing = self.routing.write().await;
        let idx = routing.advanced_cc_routes.len();
        tracing::info!(
            "Added CC route {} for device {} CC {}",
            idx,
            route.device_id.0,
            route.cc
        );
        let (Some(target), Some(param)) = (route.target, route.target_param) else {
            tracing::warn!(
                "Ignored incomplete CC route {} without target or parameter",
                idx
            );
            return idx;
        };
        routing
            .advanced_cc_routes
            .push(crate::reload::AdvancedMidiCcRoute {
                device_id: route.device_id,
                cc: route.cc,
                channel: route.channel,
                group: None,
                curve: route.curve.canonical_name().to_string(),
                target,
                param,
                min: route.range.0,
                max: route.range.1,
            });
        idx
    }

    /// Remove a keyboard route by index.
    pub async fn remove_keyboard_route(&self, index: usize) {
        let mut routing = self.routing.write().await;
        if index < routing.advanced_keyboard_routes.len() {
            routing.advanced_keyboard_routes.remove(index);
            tracing::info!("Removed keyboard route at index {}", index);
        } else {
            tracing::warn!(
                "Attempted to remove keyboard route at invalid index {}",
                index
            );
        }
    }

    /// Get the number of active keyboard routes.
    pub async fn keyboard_route_count(&self) -> usize {
        let routing = self.routing.read().await;
        routing.advanced_keyboard_routes.len() + routing.keyboard_routes.len()
    }

    /// Clear all MIDI routes (both basic and advanced).
    pub async fn clear_routes(&self) {
        let mut routing = self.routing.write().await;
        routing.keyboard_routes.clear();
        routing.cc_routes.clear();
        routing.advanced_keyboard_routes.clear();
        routing.note_routes.clear();
        routing.advanced_cc_routes.clear();
        routing.choke_groups.clear();

        {
            routing.midi2_keyboard_routes.clear();
            routing.midi2_per_note_routes.clear();
        }
        tracing::info!("Cleared all MIDI routes");
    }

    /// Add a basic keyboard route (all channels to a voice).
    pub async fn add_basic_keyboard_route(&self, device: MidiDeviceId, voice: VoiceId) {
        let mut routing = self.routing.write().await;
        routing.keyboard_routes.push(KeyboardRoute {
            device_id: device,
            channel: None, // All channels
            voice_id: voice,
        });
        tracing::info!("Routed MIDI keyboard {} to voice {}", device.0, voice.0);
    }

    /// Add a basic CC route.
    pub async fn add_basic_cc_route(&self, route: CcRoute) {
        let mut routing = self.routing.write().await;
        routing.cc_routes.push(route);
    }

    /// Apply CC routes from script state.
    ///
    /// This clears existing basic CC routes and adds the new ones.
    pub async fn apply_cc_routes(&self, routes: &[crate::reload::MidiCcRoute]) {
        let mut routing = self.routing.write().await;
        routing.cc_routes.clear();

        for route in routes {
            routing.cc_routes.push(CcRoute {
                device_id: route.device_id,
                cc: route.cc,
                target: route.target.clone(),
                param: route.param.clone(),
                min_value: route.min_value,
                max_value: route.max_value,
            });
            tracing::debug!(
                "Applied CC route: device={}, cc={}, target={:?}, param={}, min={}, max={}",
                route.device_id.0,
                route.cc,
                route.target,
                route.param,
                route.min_value,
                route.max_value
            );
        }

        if !routes.is_empty() {
            tracing::info!("Applied {} MIDI CC routes from script", routes.len());
        }
    }

    // ========================================================================
    // MIDI 1.0 Route Application (from ScriptState during reload)
    // ========================================================================

    /// Apply basic keyboard routes from script state.
    ///
    /// Clears existing basic keyboard routes and rebuilds from the provided list.
    pub async fn apply_basic_keyboard_routes(&self, routes: &[crate::reload::MidiKeyboardRoute]) {
        let mut routing = self.routing.write().await;
        routing.keyboard_routes.clear();

        for route in routes {
            routing.keyboard_routes.push(KeyboardRoute {
                device_id: route.device_id,
                channel: route.channel,
                voice_id: route.voice,
            });
            tracing::debug!(
                "Applied basic keyboard route: device={}, channel={:?}, voice={}",
                route.device_id.0,
                route.channel,
                route.voice.0
            );
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} basic MIDI keyboard routes from script",
                routes.len()
            );
        }
    }

    /// Apply advanced keyboard routes from script state.
    ///
    /// Clears existing advanced keyboard routes and rebuilds as KeyboardRouteBuilders.
    pub async fn apply_advanced_keyboard_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiKeyboardRoute],
    ) {
        let mut routing = self.routing.write().await;
        routing.advanced_keyboard_routes.clear();

        for route in routes {
            let mut builder = KeyboardRouteBuilder::new(route.device_id);
            builder.channel = route.channel;
            builder.note_min = route.note_min;
            builder.note_max = route.note_max;
            builder.transpose = route.transpose;
            builder.velocity_curve = parse_velocity_curve(&route.velocity_curve);
            builder.target_voice = Some(route.voice);

            tracing::debug!(
                "Applied advanced keyboard route: device={}, channel={:?}, notes={}-{}, transpose={}, voice={}",
                route.device_id.0,
                route.channel,
                route.note_min,
                route.note_max,
                route.transpose,
                route.voice.0
            );
            routing.advanced_keyboard_routes.push(builder);
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} advanced MIDI keyboard routes from script",
                routes.len()
            );
        }
    }

    /// Apply advanced note routes from script state.
    ///
    /// Clears existing note routes and rebuilds as NoteRouteBuilders.
    pub async fn apply_advanced_note_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiNoteRoute],
    ) {
        let mut routing = self.routing.write().await;
        routing.note_routes.clear();

        for route in routes {
            let mut builder = NoteRouteBuilder::new(route.device_id, route.source_note);
            builder.channel = route.channel;
            builder.choke_group = route.choke_group.clone();
            builder.target_voice = Some(route.voice);

            if let Some(ref param) = route.velocity_param {
                builder.velocity_mapping = Some(VelocityMapping {
                    param: param.clone(),
                    min: route.velocity_min.unwrap_or(0.0),
                    max: route.velocity_max.unwrap_or(1.0),
                });
            }

            tracing::debug!(
                "Applied advanced note route: device={}, note={}, channel={:?}, voice={}",
                route.device_id.0,
                route.source_note,
                route.channel,
                route.voice.0
            );
            routing.note_routes.push(builder);
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} advanced MIDI note routes from script",
                routes.len()
            );
        }
    }

    /// Apply advanced CC routes from script state.
    ///
    /// Clears and rebuilds the transport-transparent advanced CC registry.
    pub async fn apply_advanced_cc_routes(&self, routes: &[crate::reload::AdvancedMidiCcRoute]) {
        let mut routing = self.routing.write().await;
        routing.advanced_cc_routes.clear();

        for route in routes {
            let mut applied = route.clone();
            applied.curve = match canonical_cc_curve_name(&route.curve) {
                Some(curve) => curve.to_string(),
                None => {
                    tracing::warn!(
                        "Unknown MIDI CC curve '{}'; falling back to linear",
                        route.curve
                    );
                    "linear".to_string()
                }
            };

            if applied.curve == "logarithmic" && (applied.min <= 0.0 || applied.max <= 0.0) {
                tracing::warn!(
                    "MIDI CC logarithmic range ({}, {}) is not positive; falling back to linear",
                    applied.min,
                    applied.max
                );
            }

            if let Some(group) = applied.group {
                if !is_pipewire_midi_input_id(applied.device_id) {
                    tracing::warn!(
                        "map_cc({}).group({}) on device {} filters by UMP group but this device delivers MIDI 1; the route will never fire",
                        applied.cc,
                        group,
                        applied.device_id.0
                    );
                }
            }

            tracing::debug!(
                "Applied advanced CC route: device={}, cc={}, group={:?}, channel={:?}, target={:?}, param={}",
                route.device_id.0,
                route.cc,
                route.group,
                route.channel,
                route.target,
                route.param
            );
            routing.advanced_cc_routes.push(applied);
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} advanced MIDI CC routes from script",
                routes.len()
            );
        }
    }

    /// Apply advanced pitch-bend routes from script state.
    ///
    /// Clears existing advanced pitch-bend routes and rebuilds as CcRouteBuilders.
    pub async fn apply_advanced_bend_routes(
        &self,
        routes: &[crate::reload::AdvancedMidiBendRoute],
    ) {
        let mut routing = self.routing.write().await;
        routing.advanced_bend_routes.clear();

        for route in routes {
            let mut builder = CcRouteBuilder::new(route.device_id, 0);
            builder.channel = route.channel;
            builder.curve = parse_parameter_curve(&route.curve);
            builder.target = Some(route.target.clone());
            builder.target_param = Some(route.param.clone());
            builder.range = (route.min, route.max);

            tracing::debug!(
                "Applied advanced bend route: device={}, channel={:?}, target={:?}, param={}",
                route.device_id.0,
                route.channel,
                route.target,
                route.param
            );
            routing.advanced_bend_routes.push(builder);
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} advanced MIDI pitch-bend routes from script",
                routes.len()
            );
        }
    }

    // ========================================================================
    // MIDI 2.0 Routing
    // ========================================================================

    /// Apply MIDI 2.0 keyboard routes from script state.
    pub async fn apply_midi2_keyboard_routes(&self, routes: &[crate::reload::Midi2KeyboardRoute]) {
        let mut routing = self.routing.write().await;
        routing.midi2_keyboard_routes.clear();

        for route in routes {
            routing.midi2_keyboard_routes.push(Midi2KeyboardRoute {
                device_id: route.device_id,
                group: route.group,
                channel: route.channel,
                note_min: route.note_min,
                note_max: route.note_max,
                transpose: route.transpose,
                velocity_curve: route.velocity_curve.clone(),
                voice_id: route.voice,
            });
            tracing::debug!(
                "Applied MIDI 2.0 keyboard route: device={}, group={:?}, channel={:?}, voice={}",
                route.device_id.0,
                route.group,
                route.channel,
                route.voice.0
            );
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} MIDI 2.0 keyboard routes from script",
                routes.len()
            );
        }
    }

    /// Apply MIDI 2.0 per-note routes from script state.
    pub async fn apply_midi2_per_note_routes(&self, routes: &[crate::reload::Midi2PerNoteRoute]) {
        let mut routing = self.routing.write().await;
        routing.midi2_per_note_routes.clear();

        for route in routes {
            let controller_type = match &route.controller_type {
                crate::reload::Midi2PerNoteControllerType::PitchBend { range } => {
                    Midi2ControllerType::PitchBend { range: *range }
                }
                crate::reload::Midi2PerNoteControllerType::Pressure => {
                    Midi2ControllerType::Pressure
                }
                crate::reload::Midi2PerNoteControllerType::Timbre => Midi2ControllerType::Timbre,
                crate::reload::Midi2PerNoteControllerType::Controller(cc) => {
                    Midi2ControllerType::Controller(*cc)
                }
                crate::reload::Midi2PerNoteControllerType::RegisteredController(cc) => {
                    Midi2ControllerType::Controller(*cc)
                }
                crate::reload::Midi2PerNoteControllerType::AssignableController(cc) => {
                    Midi2ControllerType::Controller(*cc)
                }
            };

            routing.midi2_per_note_routes.push(Midi2PerNoteRoute {
                device_id: route.device_id,
                group: route.group,
                channel: route.channel,
                controller_type,
                voice_id: route.voice,
                param: route.param.clone(),
                min_value: route.min_value,
                max_value: route.max_value,
                curve: route.curve.clone(),
            });
            tracing::debug!(
                "Applied MIDI 2.0 per-note route: device={}, voice={}, param={}",
                route.device_id.0,
                route.voice.0,
                route.param
            );
        }

        if !routes.is_empty() {
            tracing::info!(
                "Applied {} MIDI 2.0 per-note routes from script",
                routes.len()
            );
        }
    }
}
