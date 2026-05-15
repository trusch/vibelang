//! Bridge between runtime MIDI events and Rhai script callbacks.
//!
//! The script registers callbacks via `mpk.on_note(|note, velocity, is_on| ...)`,
//! which stores `FnPtr`s in `vibelang_rhai::context` and `MidiCallbackConfig`s in
//! the script state. The runtime emits `MidiEventNotification`s on its callback
//! channel when matching MIDI messages arrive. This dispatcher polls that channel
//! and invokes the matching FnPtr against the script's compiled AST.

use rhai::{Dynamic, Engine, FnPtr, AST};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::mpsc;
use vibelang_core::handlers::{MidiEventNotification, MidiMessage};

/// Snapshot of the script-side callback state. Sent on every reload so the
/// dispatcher always uses the AST and FnPtrs from the most recent script run.
pub struct DispatchState {
    pub ast: AST,
    pub callbacks: HashMap<u64, FnPtr>,
}

/// Spawn the dispatcher task. Returns the sender used to push reload snapshots.
pub fn spawn(
    engine: Arc<Engine>,
    callback_rx: Arc<StdMutex<mpsc::Receiver<MidiEventNotification>>>,
    running: Arc<AtomicBool>,
) -> mpsc::Sender<DispatchState> {
    let (state_tx, mut state_rx) = mpsc::channel::<DispatchState>(8);

    tokio::spawn(async move {
        let mut current: Option<DispatchState> = None;

        while running.load(Ordering::SeqCst) {
            // Replace state with the most recent reload, if any.
            while let Ok(update) = state_rx.try_recv() {
                tracing::debug!(
                    "MIDI dispatcher: loaded {} callback(s)",
                    update.callbacks.len()
                );
                current = Some(update);
            }

            // Drain pending MIDI notifications without blocking the runtime.
            let notifications = drain(&callback_rx);

            if let Some(state) = current.as_ref() {
                for notification in notifications {
                    let Some(fnptr) = state.callbacks.get(&notification.callback_id) else {
                        continue;
                    };
                    let Some(args) = build_args(&notification.message) else {
                        continue;
                    };
                    let result: Result<Dynamic, _> = fnptr.call(engine.as_ref(), &state.ast, args);
                    if let Err(e) = result {
                        tracing::warn!("MIDI callback {} failed: {}", notification.callback_id, e);
                    }
                }
            }

            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    });

    state_tx
}

fn drain(rx: &Arc<StdMutex<mpsc::Receiver<MidiEventNotification>>>) -> Vec<MidiEventNotification> {
    let Ok(mut guard) = rx.lock() else {
        tracing::warn!("MIDI dispatcher: callback receiver mutex poisoned");
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(n) = guard.try_recv() {
        out.push(n);
    }
    out
}

/// Build the argument tuple passed to the user's Rhai closure.
///
/// Signatures (defined here, since this is the first wiring of the API):
/// - `on_note(|note, velocity, is_on|)` — note 0-127, velocity 0-127, is_on bool
/// - `on_cc(|cc, value|)` / `on_cc_num(N, |value|)` — values 0-127
/// - `on_clock_sync(|kind|)` — kind: "clock" | "start" | "stop" | "continue"
/// - `on_midi(|kind|)` for now passes the same tag string
fn build_args(msg: &MidiMessage) -> Option<Vec<Dynamic>> {
    match msg {
        MidiMessage::NoteOn { note, velocity, .. } => Some(vec![
            Dynamic::from(*note as i64),
            Dynamic::from(*velocity as i64),
            Dynamic::from(true),
        ]),
        MidiMessage::NoteOff { note, .. } => Some(vec![
            Dynamic::from(*note as i64),
            Dynamic::from(0_i64),
            Dynamic::from(false),
        ]),
        MidiMessage::ControlChange { cc, value, .. } => Some(vec![
            Dynamic::from(*cc as i64),
            Dynamic::from(*value as i64),
        ]),
        MidiMessage::PitchBend { value, .. } => Some(vec![Dynamic::from(*value as i64)]),
        MidiMessage::Clock => Some(vec![Dynamic::from("clock")]),
        MidiMessage::Start => Some(vec![Dynamic::from("start")]),
        MidiMessage::Stop => Some(vec![Dynamic::from("stop")]),
        MidiMessage::Continue => Some(vec![Dynamic::from("continue")]),
    }
}
