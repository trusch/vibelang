//! Dedicated MIDI clock output thread.
//!
//! MIDI clock is generated from monotonic absolute deadlines. Runtime transport
//! snapshots only control start/stop, tempo, and phase resets; beat jumps are
//! never converted into catch-up clock batches.

use super::output::ClockOutputChannels;
use crate::compat::Instant;
use crate::midi::QueuedMidiEvent;
use crate::transport_snapshot::TransportSnapshot;
use crate::types::ids::MidiDeviceId;
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const STOPPED_CONTROL_POLL_INTERVAL_US: u64 = 1000;
const PPQN: f64 = 24.0;
const LATE_TOLERANCE_US: u64 = 2_000;

#[derive(Clone)]
struct ClockOutput {
    device: MidiDeviceId,
    sender: Sender<QueuedMidiEvent>,
}

/// Dedicated thread for MIDI clock output.
pub struct MidiClockThread {
    /// Thread handle.
    handle: Option<JoinHandle<()>>,
    /// Running flag - set to false to stop the thread.
    running: Arc<AtomicBool>,
    /// Transport snapshot (shared with main loop).
    transport: Arc<TransportSnapshot>,
    /// Devices with clock output enabled.
    clock_devices: Arc<RwLock<HashSet<MidiDeviceId>>>,
    /// Isolated output channels for MIDI clock/control events.
    clock_channels: ClockOutputChannels,
    /// Devices waiting for a quantized Start at the next bar boundary.
    pending_starts: Arc<RwLock<HashSet<MidiDeviceId>>>,
    /// Beats per bar (from time signature, default 4).
    beats_per_bar: Arc<AtomicU8>,
    /// Incremented on enable/disable so the clock thread can refresh senders.
    device_generation: Arc<AtomicU64>,
}

impl MidiClockThread {
    /// Create a new clock thread (but don't start it yet).
    pub fn new(transport: Arc<TransportSnapshot>, clock_channels: ClockOutputChannels) -> Self {
        Self {
            handle: None,
            running: Arc::new(AtomicBool::new(false)),
            transport,
            clock_devices: Arc::new(RwLock::new(HashSet::new())),
            clock_channels,
            pending_starts: Arc::new(RwLock::new(HashSet::new())),
            beats_per_bar: Arc::new(AtomicU8::new(4)),
            device_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Set the beats per bar (from time signature numerator).
    pub fn set_beats_per_bar(&self, beats: u8) {
        self.beats_per_bar.store(beats.max(1), Ordering::Relaxed);
    }

    /// Queue a device for a quantized Start at the next bar boundary.
    pub fn queue_quantized_start(&self, device: MidiDeviceId) {
        self.pending_starts.write().insert(device);
        tracing::debug!(
            "[MIDI_CLOCK] Queued quantized Start for device {} (next bar)",
            device.0
        );
    }

    /// Start the clock thread.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            tracing::warn!("MIDI clock thread already running");
            return;
        }

        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let transport = Arc::clone(&self.transport);
        let clock_devices = Arc::clone(&self.clock_devices);
        let clock_channels = Arc::clone(&self.clock_channels);
        let pending_starts = Arc::clone(&self.pending_starts);
        let beats_per_bar = Arc::clone(&self.beats_per_bar);
        let device_generation = Arc::clone(&self.device_generation);

        let handle = thread::Builder::new()
            .name("midi-clock".to_string())
            .spawn(move || {
                run_clock_thread(
                    running,
                    transport,
                    clock_devices,
                    clock_channels,
                    pending_starts,
                    beats_per_bar,
                    device_generation,
                );
            })
            .expect("Failed to spawn MIDI clock thread");

        self.handle = Some(handle);
        tracing::info!("MIDI clock thread started");
    }

    /// Stop the clock thread and wait for it to exit.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.join() {
                tracing::warn!("MIDI clock thread panicked: {:?}", e);
            } else {
                tracing::info!("MIDI clock thread stopped");
            }
        }
    }

    /// Check if the thread is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Enable clock output for a device.
    pub fn enable_clock_output(&self, device: MidiDeviceId) {
        {
            let devices = self.clock_devices.read();
            if devices.contains(&device) {
                tracing::trace!(
                    "[MIDI_CLOCK] Clock output already enabled for device {}, skipping",
                    device.0
                );
                return;
            }
        }

        self.clock_devices.write().insert(device);
        self.device_generation.fetch_add(1, Ordering::Release);
        tracing::debug!("[MIDI_CLOCK] Enabled clock output for device {}", device.0);

        let (_beat, _tempo, playing, _generation) = self.transport.read_with_generation();
        if playing {
            send_to_device(&self.clock_channels, device, QueuedMidiEvent::Start);
        }
    }

    /// Disable clock output for a device.
    pub fn disable_clock_output(&self, device: MidiDeviceId) {
        self.clock_devices.write().remove(&device);
        self.pending_starts.write().remove(&device);
        self.device_generation.fetch_add(1, Ordering::Release);
        tracing::debug!("[MIDI_CLOCK] Disabled clock output for device {}", device.0);
    }

    /// Check if clock output is enabled for a device.
    #[allow(dead_code)]
    pub fn is_clock_enabled(&self, device: MidiDeviceId) -> bool {
        self.clock_devices.read().contains(&device)
    }

    /// Get the set of devices with clock output enabled.
    #[allow(dead_code)]
    pub fn clock_devices(&self) -> Arc<RwLock<HashSet<MidiDeviceId>>> {
        Arc::clone(&self.clock_devices)
    }
}

impl Drop for MidiClockThread {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SchedulerAction {
    start: bool,
    stop: bool,
    clock: bool,
    dropped_ticks: u64,
}

#[derive(Debug)]
struct ClockScheduler {
    next_deadline: Option<Instant>,
    tick_period: Duration,
    transport_generation: u64,
    playing: bool,
    late_tolerance: Duration,
    total_dropped_ticks: u64,
}

impl ClockScheduler {
    fn new(late_tolerance: Duration) -> Self {
        Self {
            next_deadline: None,
            tick_period: tick_period_for_tempo(120.0),
            transport_generation: 0,
            playing: false,
            late_tolerance,
            total_dropped_ticks: 0,
        }
    }

    fn poll(
        &mut self,
        now: Instant,
        tempo: f64,
        playing: bool,
        transport_generation: u64,
    ) -> SchedulerAction {
        let period = tick_period_for_tempo(tempo);

        if transport_generation != self.transport_generation {
            let was_playing = self.playing;
            self.tick_period = period;
            self.transport_generation = transport_generation;
            self.playing = playing;
            self.next_deadline = playing.then_some(now + self.tick_period);

            return SchedulerAction {
                start: playing && !was_playing,
                stop: !playing && was_playing,
                ..SchedulerAction::default()
            };
        }

        self.tick_period = period;
        self.playing = playing;

        if !playing {
            self.next_deadline = None;
            return SchedulerAction::default();
        }

        let Some(deadline) = self.next_deadline else {
            self.next_deadline = Some(now + self.tick_period);
            return SchedulerAction::default();
        };

        if now < deadline {
            return SchedulerAction::default();
        }

        let mut next_deadline = deadline + self.tick_period;
        let mut dropped_ticks = 0;

        if now.duration_since(deadline) > self.late_tolerance {
            while next_deadline <= now {
                dropped_ticks += 1;
                next_deadline += self.tick_period;
            }
            self.total_dropped_ticks += dropped_ticks;
        }

        self.next_deadline = Some(next_deadline);

        SchedulerAction {
            clock: true,
            dropped_ticks,
            ..SchedulerAction::default()
        }
    }

    fn sleep_duration(&self, now: Instant) -> Duration {
        match self.next_deadline {
            Some(deadline) if deadline > now => deadline.duration_since(now),
            Some(_) => Duration::ZERO,
            None => Duration::from_micros(STOPPED_CONTROL_POLL_INTERVAL_US),
        }
    }
}

/// Tolerance for bar boundary detection (in beats).
const BAR_BOUNDARY_TOLERANCE: f64 = 0.02;

fn run_clock_thread(
    running: Arc<AtomicBool>,
    transport: Arc<TransportSnapshot>,
    clock_devices: Arc<RwLock<HashSet<MidiDeviceId>>>,
    clock_channels: ClockOutputChannels,
    pending_starts: Arc<RwLock<HashSet<MidiDeviceId>>>,
    beats_per_bar: Arc<AtomicU8>,
    device_generation: Arc<AtomicU64>,
) {
    tracing::info!("[MIDI_CLOCK] Thread started with absolute-deadline scheduler");

    let mut scheduler = ClockScheduler::new(Duration::from_micros(LATE_TOLERANCE_US));
    let mut cached_outputs = Vec::new();
    let mut cached_device_generation = u64::MAX;

    while running.load(Ordering::Relaxed) {
        let now = Instant::now();
        let (beat, tempo, playing, transport_generation) = transport.read_with_generation();

        let current_device_generation = device_generation.load(Ordering::Acquire);
        if current_device_generation != cached_device_generation {
            cached_outputs = refresh_clock_outputs(&clock_devices, &clock_channels);
            cached_device_generation = current_device_generation;
        }

        let action = scheduler.poll(now, tempo, playing, transport_generation);

        if action.start {
            send_to_outputs(&cached_outputs, QueuedMidiEvent::Start);
            tracing::debug!("[MIDI_CLOCK] Transport started at beat {}", beat);
        }
        if action.stop {
            send_to_outputs(&cached_outputs, QueuedMidiEvent::Stop);
            tracing::debug!("[MIDI_CLOCK] Transport stopped");
        }
        if action.dropped_ticks > 0 {
            tracing::warn!(
                "[MIDI_CLOCK] Dropped {} late clock tick(s), total dropped={}",
                action.dropped_ticks,
                scheduler.total_dropped_ticks
            );
        }
        if action.clock {
            let output_drops = send_to_outputs(&cached_outputs, QueuedMidiEvent::Clock);
            if output_drops > 0 {
                tracing::warn!(
                    "[MIDI_CLOCK] Dropped {} clock event(s) because output channel was full",
                    output_drops
                );
            }
        }

        send_pending_quantized_starts(
            beat,
            playing,
            &pending_starts,
            &beats_per_bar,
            &clock_channels,
        );

        let sleep_for = scheduler.sleep_duration(Instant::now());
        if sleep_for.is_zero() {
            thread::yield_now();
        } else {
            thread::park_timeout(sleep_for);
        }
    }

    tracing::info!("[MIDI_CLOCK] Thread exiting");
}

fn tick_period_for_tempo(tempo: f64) -> Duration {
    let tempo = tempo.clamp(1.0, 1000.0);
    Duration::from_secs_f64(60.0 / (tempo * PPQN))
}

fn refresh_clock_outputs(
    clock_devices: &Arc<RwLock<HashSet<MidiDeviceId>>>,
    clock_channels: &ClockOutputChannels,
) -> Vec<ClockOutput> {
    let devices = clock_devices.read();
    if devices.is_empty() {
        return Vec::new();
    }

    let Ok(channels) = clock_channels.lock() else {
        tracing::warn!("[MIDI_CLOCK] Failed to lock clock channels (mutex poisoned)");
        return Vec::new();
    };

    devices
        .iter()
        .filter_map(|device| {
            channels.get(device).map(|sender| ClockOutput {
                device: *device,
                sender: sender.clone(),
            })
        })
        .collect()
}

fn send_to_outputs(outputs: &[ClockOutput], event: QueuedMidiEvent) -> u64 {
    let mut dropped = 0;
    for output in outputs {
        if let Err(e) = output.sender.try_send(event.clone()) {
            dropped += 1;
            tracing::trace!(
                "[MIDI_CLOCK] Failed to send {:?} to device {}: {}",
                event,
                output.device.0,
                e
            );
        }
    }
    dropped
}

fn send_to_device(
    clock_channels: &ClockOutputChannels,
    device: MidiDeviceId,
    event: QueuedMidiEvent,
) -> bool {
    let Ok(channels) = clock_channels.lock() else {
        tracing::warn!("[MIDI_CLOCK] Failed to lock clock channels (mutex poisoned)");
        return false;
    };

    let Some(sender) = channels.get(&device) else {
        return false;
    };

    if let Err(e) = sender.try_send(event) {
        tracing::warn!(
            "[MIDI_CLOCK] Failed to send clock/control event to device {}: {}",
            device.0,
            e
        );
        return false;
    }

    true
}

fn send_pending_quantized_starts(
    beat: f64,
    playing: bool,
    pending_starts: &Arc<RwLock<HashSet<MidiDeviceId>>>,
    beats_per_bar: &Arc<AtomicU8>,
    clock_channels: &ClockOutputChannels,
) {
    if !playing || pending_starts.read().is_empty() {
        return;
    }

    let bar_len = beats_per_bar.load(Ordering::Relaxed) as f64;
    let position_in_bar = beat % bar_len;

    if position_in_bar >= BAR_BOUNDARY_TOLERANCE
        && position_in_bar <= (bar_len - BAR_BOUNDARY_TOLERANCE)
    {
        return;
    }

    let devices: Vec<MidiDeviceId> = pending_starts.write().drain().collect();
    for device in devices {
        if send_to_device(clock_channels, device, QueuedMidiEvent::Start) {
            tracing::info!(
                "[MIDI_CLOCK] Sent quantized Start to device {} at bar boundary (beat {})",
                device.0,
                beat
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::looper::{LooperAction, LooperManager};
    use crate::reload::LooperConfig;
    use crate::types::ids::VoiceId;
    use std::collections::HashMap;

    fn clock_channels() -> ClockOutputChannels {
        Arc::new(std::sync::Mutex::new(HashMap::new()))
    }

    #[test]
    fn test_clock_thread_lifecycle() {
        let transport = Arc::new(TransportSnapshot::new());
        let clock_channels = clock_channels();

        let mut clock_thread = MidiClockThread::new(transport, clock_channels);

        assert!(!clock_thread.is_running());

        clock_thread.start();
        assert!(clock_thread.is_running());

        std::thread::sleep(Duration::from_millis(10));

        clock_thread.stop();
        assert!(!clock_thread.is_running());
    }

    #[test]
    fn test_enable_disable_clock() {
        let transport = Arc::new(TransportSnapshot::new());
        let clock_channels = clock_channels();

        let clock_thread = MidiClockThread::new(transport, clock_channels);

        let device = MidiDeviceId(1);

        assert!(!clock_thread.is_clock_enabled(device));

        clock_thread.enable_clock_output(device);
        assert!(clock_thread.is_clock_enabled(device));

        clock_thread.disable_clock_output(device);
        assert!(!clock_thread.is_clock_enabled(device));
    }

    #[test]
    fn stall_drops_missed_ticks_without_bursting() {
        let mut scheduler = ClockScheduler::new(Duration::from_micros(LATE_TOLERANCE_US));
        let start = Instant::now();
        let period = tick_period_for_tempo(120.0);

        let action = scheduler.poll(start, 120.0, true, 1);
        assert!(action.start);
        assert!(!action.clock);

        let stalled_now = start + period + Duration::from_millis(100);
        let action = scheduler.poll(stalled_now, 120.0, true, 1);

        assert!(action.clock);
        assert!(action.dropped_ticks >= 4);
        assert!(scheduler.next_deadline.unwrap() > stalled_now);

        let same_timestamp_sends = u8::from(action.clock);
        assert!(same_timestamp_sends <= 1);
    }

    #[test]
    fn generation_reset_does_not_emit_missed_pulses() {
        let mut scheduler = ClockScheduler::new(Duration::from_micros(LATE_TOLERANCE_US));
        let start = Instant::now();
        let period = tick_period_for_tempo(120.0);

        assert!(scheduler.poll(start, 120.0, true, 1).start);
        let action = scheduler.poll(start + Duration::from_millis(500), 120.0, true, 2);
        assert!(!action.clock);
        assert_eq!(action.dropped_ticks, 0);
        assert_eq!(
            scheduler.next_deadline,
            Some(start + Duration::from_millis(500) + period)
        );
    }

    #[test]
    fn start_sends_once_and_resets_deadline() {
        let mut scheduler = ClockScheduler::new(Duration::from_micros(LATE_TOLERANCE_US));
        let start = Instant::now();
        let action = scheduler.poll(start, 120.0, true, 1);

        assert_eq!(
            action,
            SchedulerAction {
                start: true,
                stop: false,
                clock: false,
                dropped_ticks: 0,
            }
        );
        assert_eq!(
            scheduler.next_deadline,
            Some(start + tick_period_for_tempo(120.0))
        );
    }

    #[test]
    fn two_looper_finalize_load_still_keeps_clock_min_spaced() {
        let mut manager = LooperManager::new();
        let configs = [
            LooperConfig {
                device_id: MidiDeviceId::new(1),
                voice_id: VoiceId::new(1),
                channel: None,
                silence_bars: 0.25,
                quantize_beats: 0.25,
            },
            LooperConfig {
                device_id: MidiDeviceId::new(2),
                voice_id: VoiceId::new(2),
                channel: None,
                silence_bars: 0.25,
                quantize_beats: 0.25,
            },
        ];
        manager.reconcile(&configs);
        manager.handle_note_on(MidiDeviceId::new(1), 0, 60, 100, 0.0, 4);
        manager.handle_note_on(MidiDeviceId::new(2), 0, 64, 100, 0.0, 4);

        let actions = manager.tick(2.1, 4);
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, LooperAction::StartPattern { .. }))
                .count(),
            2
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, LooperAction::NoteOff { .. }))
                .count(),
            2
        );

        let mut scheduler = ClockScheduler::new(Duration::from_micros(LATE_TOLERANCE_US));
        let start = Instant::now();
        let period = tick_period_for_tempo(120.0);
        scheduler.poll(start, 120.0, true, 1);

        let mut send_times = Vec::new();
        let action = scheduler.poll(start + period, 120.0, true, 1);
        if action.clock {
            send_times.push(start + period);
        }

        let after_looper_stall = start + period + Duration::from_millis(100);
        let action = scheduler.poll(after_looper_stall, 120.0, true, 1);
        if action.clock {
            send_times.push(after_looper_stall);
        }

        assert_eq!(send_times.len(), 2);
        assert!(send_times[1].duration_since(send_times[0]) >= period);
        assert!(action.dropped_ticks > 0);
    }
}
