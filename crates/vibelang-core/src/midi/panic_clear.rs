use std::time::{Duration, Instant};

#[cfg(feature = "midi")]
use crossbeam_channel::{Receiver, Sender};
#[cfg(feature = "midi")]
use midir::MidiOutputConnection;
#[cfg(feature = "midi")]
use std::thread::{self, JoinHandle};

const MIDI_CHANNEL_COUNT: usize = 16;
const PANIC_CLEAR_CONTROLLERS: [u8; 3] = [64, 123, 120];
const PANIC_CLEAR_MESSAGE_BYTES: usize = 3;
const PANIC_CLEAR_MESSAGE_COUNT: usize = MIDI_CHANNEL_COUNT * PANIC_CLEAR_CONTROLLERS.len();
const PANIC_CLEAR_TOTAL_BYTES: usize = PANIC_CLEAR_MESSAGE_COUNT * PANIC_CLEAR_MESSAGE_BYTES;

/// Three bytes per 10 ms is 300 bytes/s: below DIN MIDI's 3,125 bytes/s and
/// safely below the 17-byte synchronous burst that wedged the Model 15.
const PANIC_CLEAR_GAP: Duration = Duration::from_millis(10);

#[cfg(feature = "midi")]
const LIVE_OUTPUT_QUEUE_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MidiOutputOpenProfile {
    PacedClear,
    SideEffectFree,
}

impl MidiOutputOpenProfile {
    /// `midir` formats ALSA names as `<client>:<port> <client-id>:<port-id>`.
    /// Match only the exact canonical client component so similarly named
    /// devices do not silently lose their open-time clear.
    pub(crate) fn for_device_name(device_name: &str) -> Self {
        let client_name = device_name.split(':').next().unwrap_or(device_name);
        let canonical_client_name: String = client_name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .map(|character| character.to_ascii_uppercase())
            .collect();

        if canonical_client_name == "MODEL15" {
            Self::SideEffectFree
        } else {
            Self::PacedClear
        }
    }

    fn sends_automatic_clear(self) -> bool {
        matches!(self, Self::PacedClear)
    }
}

#[derive(Debug, Eq, PartialEq)]
enum PanicClearAction {
    Live(Vec<u8>),
    Clear { index: usize, message: [u8; 3] },
    Wait(Duration),
    Complete,
    Cancelled,
}

struct PanicClearSchedule {
    next_index: usize,
    next_clear_at: Instant,
    gap: Duration,
    profile: MidiOutputOpenProfile,
    cancelled: bool,
}

impl PanicClearSchedule {
    fn new(now: Instant, gap: Duration, profile: MidiOutputOpenProfile) -> Self {
        Self {
            next_index: 0,
            next_clear_at: now + gap,
            gap,
            profile,
            cancelled: false,
        }
    }

    fn next_action(&mut self, now: Instant, live: Option<Vec<u8>>) -> PanicClearAction {
        if self.cancelled {
            return PanicClearAction::Cancelled;
        }

        if let Some(message) = live {
            if self.next_index < PANIC_CLEAR_MESSAGE_COUNT {
                self.next_clear_at = now + self.gap;
            }
            return PanicClearAction::Live(message);
        }

        if !self.profile.sends_automatic_clear() {
            return PanicClearAction::Complete;
        }

        if self.next_index == PANIC_CLEAR_MESSAGE_COUNT {
            return PanicClearAction::Complete;
        }

        if now < self.next_clear_at {
            return PanicClearAction::Wait(self.next_clear_at - now);
        }

        let index = self.next_index;
        let channel = index / PANIC_CLEAR_CONTROLLERS.len();
        let controller = PANIC_CLEAR_CONTROLLERS[index % PANIC_CLEAR_CONTROLLERS.len()];
        self.next_index += 1;
        self.next_clear_at = now + self.gap;

        PanicClearAction::Clear {
            index,
            message: [0xB0 | channel as u8, controller, 0],
        }
    }

    fn cancel(&mut self) {
        self.cancelled = true;
    }

    fn sent(&self) -> usize {
        self.next_index
    }
}

#[cfg(feature = "midi")]
trait MidiOutputSink: Send + 'static {
    fn send(&mut self, message: &[u8]) -> Result<(), String>;
}

#[cfg(feature = "midi")]
impl MidiOutputSink for MidiOutputConnection {
    fn send(&mut self, message: &[u8]) -> Result<(), String> {
        MidiOutputConnection::send(self, message).map_err(|error| error.to_string())
    }
}

/// Owns one MIDI output connection and sends its bounded open-time clear on a
/// background thread. Live messages use a separate bounded queue and always
/// take priority over the next clear message.
#[cfg(feature = "midi")]
pub(crate) struct PacedPanicClearOutput {
    live_tx: Sender<Vec<u8>>,
    cancel_tx: Sender<()>,
    worker: Option<JoinHandle<()>>,
    device_label: String,
}

#[cfg(feature = "midi")]
impl PacedPanicClearOutput {
    pub(crate) fn new(
        connection: MidiOutputConnection,
        device_label: impl Into<String>,
        profile: MidiOutputOpenProfile,
    ) -> Result<Self, String> {
        Self::spawn(connection, device_label.into(), PANIC_CLEAR_GAP, profile)
    }

    fn spawn<S: MidiOutputSink>(
        sink: S,
        device_label: String,
        gap: Duration,
        profile: MidiOutputOpenProfile,
    ) -> Result<Self, String> {
        let (live_tx, live_rx) = crossbeam_channel::bounded(LIVE_OUTPUT_QUEUE_CAPACITY);
        let (cancel_tx, cancel_rx) = crossbeam_channel::bounded(1);
        let worker_label = device_label.clone();
        let worker = thread::Builder::new()
            .name("midi-panic-clear".to_string())
            .spawn(move || run_output_worker(sink, live_rx, cancel_rx, worker_label, gap, profile))
            .map_err(|error| format!("Failed to start MIDI panic-clear worker: {error}"))?;

        Ok(Self {
            live_tx,
            cancel_tx,
            worker: Some(worker),
            device_label,
        })
    }

    pub(crate) fn send(&self, message: &[u8]) -> Result<(), String> {
        self.live_tx
            .try_send(message.to_vec())
            .map_err(|error| format!("MIDI output {} queue failed: {error}", self.device_label))
    }
}

#[cfg(feature = "midi")]
impl Drop for PacedPanicClearOutput {
    fn drop(&mut self) {
        let _ = self.cancel_tx.try_send(());
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!(
                    "panic-clear [{}]: output worker panicked during shutdown",
                    self.device_label
                );
            }
        }
    }
}

#[cfg(feature = "midi")]
fn run_output_worker<S: MidiOutputSink>(
    mut sink: S,
    live_rx: Receiver<Vec<u8>>,
    cancel_rx: Receiver<()>,
    device_label: String,
    gap: Duration,
    profile: MidiOutputOpenProfile,
) {
    let mut schedule = PanicClearSchedule::new(Instant::now(), gap, profile);
    let mut completion_logged = !profile.sends_automatic_clear();
    match profile {
        MidiOutputOpenProfile::PacedClear => tracing::info!(
            "panic-clear [{}]: scheduled {} messages / {} bytes, {}-byte chunks, {} ms gap",
            device_label,
            PANIC_CLEAR_MESSAGE_COUNT,
            PANIC_CLEAR_TOTAL_BYTES,
            PANIC_CLEAR_MESSAGE_BYTES,
            gap.as_millis()
        ),
        MidiOutputOpenProfile::SideEffectFree => tracing::info!(
            "panic-clear [{}]: disabled by side-effect-free output-open profile",
            device_label
        ),
    }

    loop {
        if cancel_rx.try_recv().is_ok() {
            schedule.cancel();
        }

        let live = if schedule.cancelled {
            None
        } else {
            live_rx.try_recv().ok()
        };
        match schedule.next_action(Instant::now(), live) {
            PanicClearAction::Live(message) => send_live(&mut sink, &device_label, &message),
            PanicClearAction::Clear { index, message } => {
                if let Err(error) = sink.send(&message) {
                    tracing::warn!(
                        "panic-clear [{}]: message {}/{} failed: {}",
                        device_label,
                        index + 1,
                        PANIC_CLEAR_MESSAGE_COUNT,
                        error
                    );
                } else {
                    tracing::trace!(
                        "panic-clear [{}]: sent {}/{} {:02x?}",
                        device_label,
                        index + 1,
                        PANIC_CLEAR_MESSAGE_COUNT,
                        message
                    );
                }
            }
            PanicClearAction::Wait(wait) => {
                crossbeam_channel::select_biased! {
                    recv(cancel_rx) -> _ => schedule.cancel(),
                    recv(live_rx) -> message => {
                        if let Ok(message) = message {
                            if let PanicClearAction::Live(message) =
                                schedule.next_action(Instant::now(), Some(message))
                            {
                                send_live(&mut sink, &device_label, &message);
                            }
                        }
                    },
                    default(wait) => {}
                }
            }
            PanicClearAction::Complete => {
                if !completion_logged {
                    tracing::info!(
                        "panic-clear [{}]: completed {} messages / {} bytes",
                        device_label,
                        PANIC_CLEAR_MESSAGE_COUNT,
                        PANIC_CLEAR_TOTAL_BYTES
                    );
                    completion_logged = true;
                }
                crossbeam_channel::select_biased! {
                    recv(cancel_rx) -> _ => schedule.cancel(),
                    recv(live_rx) -> message => {
                        if let Ok(message) = message {
                            send_live(&mut sink, &device_label, &message);
                        }
                    }
                }
            }
            PanicClearAction::Cancelled => {
                for message in live_rx.try_iter() {
                    send_live(&mut sink, &device_label, &message);
                }
                tracing::info!(
                    "panic-clear [{}]: cancelled after {}/{} messages",
                    device_label,
                    schedule.sent(),
                    PANIC_CLEAR_MESSAGE_COUNT
                );
                return;
            }
        }
    }
}

#[cfg(feature = "midi")]
fn send_live<S: MidiOutputSink>(sink: &mut S, device_label: &str, message: &[u8]) {
    if let Err(error) = sink.send(message) {
        tracing::warn!("MIDI output [{}] send failed: {}", device_label, error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_clear_order_and_pacing_stay_below_usb_failure_boundary() {
        let start = Instant::now();
        let mut schedule =
            PanicClearSchedule::new(start, PANIC_CLEAR_GAP, MidiOutputOpenProfile::PacedClear);
        let mut now = start;
        let mut sent = Vec::new();

        while sent.len() < PANIC_CLEAR_MESSAGE_COUNT {
            match schedule.next_action(now, None) {
                PanicClearAction::Wait(wait) => now += wait,
                PanicClearAction::Clear { message, .. } => sent.push((now, message)),
                action => panic!("unexpected action: {action:?}"),
            }
        }

        let expected: Vec<[u8; 3]> = (0..16u8)
            .flat_map(|channel| {
                let status = 0xB0 | channel;
                [[status, 64, 0], [status, 123, 0], [status, 120, 0]]
            })
            .collect();

        assert_eq!(
            sent.iter().map(|(_, message)| *message).collect::<Vec<_>>(),
            expected
        );
        assert!(sent
            .windows(2)
            .all(|pair| pair[1].0.duration_since(pair[0].0) >= PANIC_CLEAR_GAP));
        assert!(sent.iter().all(|(_, message)| message.len() < 17));

        let paced_bytes_per_second =
            PANIC_CLEAR_MESSAGE_BYTES as u128 * 1_000 / PANIC_CLEAR_GAP.as_millis();
        assert!(paced_bytes_per_second < 3_125);
        assert_eq!(sent.len() * PANIC_CLEAR_MESSAGE_BYTES, 144);
        assert_eq!(
            sent.last().unwrap().0.duration_since(start),
            PANIC_CLEAR_GAP * PANIC_CLEAR_MESSAGE_COUNT as u32
        );
    }

    #[test]
    fn cancelled_schedule_never_emits_stale_clear() {
        let start = Instant::now();
        let mut schedule =
            PanicClearSchedule::new(start, PANIC_CLEAR_GAP, MidiOutputOpenProfile::PacedClear);

        assert!(matches!(
            schedule.next_action(start + PANIC_CLEAR_GAP, None),
            PanicClearAction::Clear { index: 0, .. }
        ));
        schedule.cancel();

        assert_eq!(
            schedule.next_action(start + PANIC_CLEAR_GAP * 100, None),
            PanicClearAction::Cancelled
        );
        assert_eq!(schedule.sent(), 1);
    }

    #[test]
    fn live_message_preempts_due_clear_and_defers_it() {
        let start = Instant::now();
        let due = start + PANIC_CLEAR_GAP;
        let note_on = vec![0x90, 60, 100];
        let mut schedule =
            PanicClearSchedule::new(start, PANIC_CLEAR_GAP, MidiOutputOpenProfile::PacedClear);

        assert_eq!(
            schedule.next_action(due, Some(note_on.clone())),
            PanicClearAction::Live(note_on)
        );
        assert_eq!(schedule.sent(), 0);
        assert_eq!(
            schedule.next_action(due, None),
            PanicClearAction::Wait(PANIC_CLEAR_GAP)
        );
        assert!(matches!(
            schedule.next_action(due + PANIC_CLEAR_GAP, None),
            PanicClearAction::Clear { index: 0, .. }
        ));
    }

    #[test]
    fn model_15_alsa_client_selects_side_effect_free_profile() {
        for name in [
            "MODEL15:MODEL15 MIDI 1 36:0",
            "MODEL 15:MODEL 15 MIDI 1 36:0",
            "model15",
        ] {
            assert_eq!(
                MidiOutputOpenProfile::for_device_name(name),
                MidiOutputOpenProfile::SideEffectFree,
                "unexpected profile for {name}"
            );
        }
    }

    #[test]
    fn other_device_names_retain_paced_clear_profile() {
        for name in [
            "MPD232:MPD232 MIDI 1 20:0",
            "EP-133:EP-133 MIDI 1 40:0",
            "OTHER:MODEL15 MIDI 1 36:0",
            "MODEL15 Clone:MODEL15 MIDI 1 36:0",
            "MODEL150:MODEL150 MIDI 1 36:0",
        ] {
            assert_eq!(
                MidiOutputOpenProfile::for_device_name(name),
                MidiOutputOpenProfile::PacedClear,
                "unexpected profile for {name}"
            );
        }
    }

    #[test]
    fn side_effect_free_schedule_emits_no_automatic_clear() {
        let start = Instant::now();
        let mut schedule = PanicClearSchedule::new(
            start,
            PANIC_CLEAR_GAP,
            MidiOutputOpenProfile::SideEffectFree,
        );

        assert_eq!(
            schedule.next_action(start + PANIC_CLEAR_GAP * 100, None),
            PanicClearAction::Complete
        );
        assert_eq!(schedule.sent(), 0);
    }

    #[cfg(feature = "midi")]
    mod worker_tests {
        use super::*;
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct RecordingSink(Arc<Mutex<Vec<Vec<u8>>>>);

        impl MidiOutputSink for RecordingSink {
            fn send(&mut self, message: &[u8]) -> Result<(), String> {
                self.0.lock().unwrap().push(message.to_vec());
                Ok(())
            }
        }

        struct ChannelSink(Sender<Vec<u8>>);

        impl MidiOutputSink for ChannelSink {
            fn send(&mut self, message: &[u8]) -> Result<(), String> {
                self.0
                    .send(message.to_vec())
                    .map_err(|error| error.to_string())
            }
        }

        #[test]
        fn side_effect_free_worker_sends_only_live_messages() {
            let (sent_tx, sent_rx) = crossbeam_channel::unbounded();
            let output = PacedPanicClearOutput::spawn(
                ChannelSink(sent_tx),
                "MODEL15".to_string(),
                Duration::ZERO,
                MidiOutputOpenProfile::SideEffectFree,
            )
            .unwrap();

            assert!(sent_rx.recv_timeout(Duration::from_millis(20)).is_err());

            let note_on = vec![0x90, 60, 100];
            output.send(&note_on).unwrap();
            assert_eq!(
                sent_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
                note_on
            );
            assert!(sent_rx.try_recv().is_err());
            drop(output);
            assert!(sent_rx.try_recv().is_err());
        }

        #[test]
        fn paced_clear_worker_still_sends_the_complete_clear() {
            let (sent_tx, sent_rx) = crossbeam_channel::unbounded();
            let output = PacedPanicClearOutput::spawn(
                ChannelSink(sent_tx),
                "EP-133".to_string(),
                Duration::ZERO,
                MidiOutputOpenProfile::PacedClear,
            )
            .unwrap();

            let sent: Vec<Vec<u8>> = (0..PANIC_CLEAR_MESSAGE_COUNT)
                .map(|_| sent_rx.recv_timeout(Duration::from_secs(1)).unwrap())
                .collect();
            let expected: Vec<Vec<u8>> = (0..16u8)
                .flat_map(|channel| {
                    let status = 0xB0 | channel;
                    [
                        vec![status, 64, 0],
                        vec![status, 123, 0],
                        vec![status, 120, 0],
                    ]
                })
                .collect();

            assert_eq!(sent, expected);
            assert!(sent_rx.try_recv().is_err());
            drop(output);
        }

        #[test]
        fn open_is_non_blocking_and_close_reopen_cancels_stale_work() {
            let recorded = Arc::new(Mutex::new(Vec::new()));
            let sink = RecordingSink(Arc::clone(&recorded));
            let first = PacedPanicClearOutput::spawn(
                sink.clone(),
                "first".to_string(),
                Duration::from_secs(60),
                MidiOutputOpenProfile::PacedClear,
            )
            .unwrap();

            first.send(&[0x90, 60, 100]).unwrap();
            drop(first);
            assert_eq!(*recorded.lock().unwrap(), vec![vec![0x90, 60, 100]]);

            let second = PacedPanicClearOutput::spawn(
                sink.clone(),
                "second".to_string(),
                Duration::from_secs(60),
                MidiOutputOpenProfile::PacedClear,
            )
            .unwrap();
            second.send(&[0x80, 60, 0]).unwrap();
            drop(second);
            assert_eq!(
                *recorded.lock().unwrap(),
                vec![vec![0x90, 60, 100], vec![0x80, 60, 0]]
            );
        }
    }
}
