//! Message handlers for the VibeLang runtime.
//!
//! This module contains handlers for different categories of StateMessage variants.
//! Each handler module receives a `RuntimeContext` with references to the shared state
//! and communication channels needed to process messages.
//!
//! # Module Structure
//!
//! - `transport` - Transport control (play, stop, tempo, time signature)
//! - `groups` - Group management (create, delete, mute, solo)
//! - `voices` - Voice management (create, trigger, delete)
//! - `patterns` - Pattern scheduling
//! - `melodies` - Melody scheduling
//! - `midi` - MIDI input/output processing
//! - `recording` - Audio recording

pub mod effects;
pub mod groups;
pub mod melodies;
pub mod midi_input;
pub mod midi_output;
pub mod patterns;
pub mod synthdef;
pub mod transport;
pub mod voices;

use crate::midi_realtime::MidiRealtimeService;
use crate::osc_sender::OscSender;
use crate::reload::ReloadManager;
use crate::scheduler::EventScheduler;
use crate::scsynth::Scsynth;
use crate::state::StateManager;
use crate::timing::TransportClock;

/// Context struct providing access to runtime components for message handlers.
///
/// This struct holds references to all the shared state and services that handlers
/// need to process messages. It avoids passing many parameters to each handler
/// function and makes it clear what dependencies handlers have access to.
pub struct RuntimeContext<'a> {
    /// Centralized OSC sender - handles all scsynth communication and score capture.
    pub osc_sender: &'a mut OscSender,
    /// Scsynth connection for receiving responses.
    pub sc: &'a Scsynth,
    /// Shared state manager.
    pub shared: &'a StateManager,
    /// Event scheduler.
    pub scheduler: &'a mut EventScheduler,
    /// Transport clock for timing.
    pub transport: &'a mut TransportClock,
    /// Live reload manager.
    pub reload_manager: &'a mut ReloadManager,
    /// Completion channel for play_once sequences.
    pub completion_tx: &'a Option<crossbeam_channel::Sender<String>>,
    /// Node ID for the SC-managed MIDI clock synth.
    pub sc_midi_clock_node_id: &'a mut Option<i32>,
    /// MIDI realtime service for output processing.
    pub midi_rt: &'a mut MidiRealtimeService,
}

impl<'a> RuntimeContext<'a> {
    /// Send a MIDI clock message to configured output devices.
    pub fn send_midi_clock_message(&self, event: crate::midi::QueuedMidiEvent) {
        self.shared.with_state_read(|state| {
            let config = &state.midi_output_config;
            if let Some(target_id) = config.clock_device_id {
                // Send to specific device
                if let Some(device) = config.devices.get(&target_id) {
                    let _ = device.event_tx.send(event.clone());
                }
            } else {
                // Send to all devices
                for device in config.devices.values() {
                    let _ = device.event_tx.send(event.clone());
                }
            }
        });
    }
}
