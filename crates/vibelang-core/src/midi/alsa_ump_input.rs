//! ALSA raw UMP MIDI 2.0 input support (Linux).
//!
//! Reads UMP packets straight from a kernel UMP rawmidi endpoint
//! (`/dev/snd/umpC<card>D<device>`), the node created by `CONFIG_SND_UMP`
//! for a USB MIDI 2.0 device. This is the transport counterpart to
//! [`super::pipewire_input`]: same `UmpParser` and event path, no PipeWire.
//!
//! Why it exists as its own path rather than going through the sequencer:
//! the kernel types the UMP *endpoint* sequencer port `MIDI_UMP | HARDWARE |
//! PORT` **without** `MIDI_GENERIC` (`sound/core/seq/seq_ump_client.c`), and
//! midir — like most legacy clients — enumerates only ports matching
//! `MIDI_GENERIC | SYNTH | APPLICATION`, so it cannot see it. The per-group
//! ports that *are* legacy-visible carry MIDI 1.0-converted data, which
//! discards exactly the resolution MIDI 2.0 exists for (32-bit controllers,
//! 16-bit velocity, per-note controllers). Reading the endpoint node keeps
//! the full-resolution stream.
//!
//! Devices are identified by their `/dev/snd` node name, so ids are stable
//! across reopen for the same card/device pair.

use crate::midi::{MidiClock, MidiEventSender, TimestampedMidiEvent, UmpParser};
use crate::types::ids::MidiDeviceId;
use std::sync::Arc;

/// Marks a [`MidiDeviceId`] as an ALSA UMP endpoint. Distinct from
/// [`super::pipewire_input::PIPEWIRE_MIDI_INPUT_FLAG`] so the two transports
/// can coexist in one device list.
pub const ALSA_UMP_INPUT_FLAG: u32 = 0x4000_0000;

/// Mirror of `pipewire_input::PIPEWIRE_MIDI_INPUT_FLAG`, kept local so ids
/// stay disjoint even when that module is compiled out.
const PIPEWIRE_MIDI_INPUT_FLAG_BIT: u32 = 0x8000_0000;

#[derive(Clone, Debug)]
pub struct AlsaUmpInputInfo {
    pub id: MidiDeviceId,
    /// Human-readable name, e.g. `"Gamma (UMP)"`.
    pub name: String,
    /// Device node path, e.g. `/dev/snd/umpC1D0`.
    pub node: String,
}

pub fn is_alsa_ump_input_id(id: MidiDeviceId) -> bool {
    id.raw() & ALSA_UMP_INPUT_FLAG != 0
}

/// Derive a stable id from the device node name (FNV-1a, flagged).
pub fn alsa_ump_input_id(node: &str) -> MidiDeviceId {
    let mut hash = 0x811C_9DC5u32;
    for byte in node.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    // Keep both transport flag bits clear in the payload so the tag is
    // unambiguous. The PipeWire bit is spelled out rather than imported
    // because that module is compiled out without its feature.
    let payload = hash & !(ALSA_UMP_INPUT_FLAG | PIPEWIRE_MIDI_INPUT_FLAG_BIT);
    MidiDeviceId::new(ALSA_UMP_INPUT_FLAG | payload)
}

/// `"umpC1D0"` -> `Some((1, 0))`.
fn parse_node_name(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_prefix("umpC")?;
    let (card, device) = rest.split_once('D')?;
    Some((card.parse().ok()?, device.parse().ok()?))
}

/// Card name from `/proc/asound/card<N>/id`, falling back to the node name.
fn card_name(card: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/asound/card{card}/id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Enumerate UMP endpoints exposed by the kernel. Empty when the kernel
/// lacks `CONFIG_SND_UMP`, when no MIDI 2.0 device is attached, or on
/// non-Linux hosts.
pub fn list_alsa_ump_inputs() -> Vec<AlsaUmpInputInfo> {
    let mut found: Vec<AlsaUmpInputInfo> = std::fs::read_dir("/dev/snd")
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let (card, device) = parse_node_name(&file_name)?;
            let node = format!("/dev/snd/{file_name}");
            let name = match card_name(card) {
                Some(card_id) => format!("{card_id} (UMP)"),
                None => format!("UMP {card}:{device}"),
            };
            Some(AlsaUmpInputInfo {
                id: alsa_ump_input_id(&node),
                name,
                node,
            })
        })
        .collect();
    found.sort_by(|a, b| a.node.cmp(&b.node));
    found
}

/// Open handle; dropping it stops the reader thread.
pub struct AlsaUmpInputConnection {
    running: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AlsaUmpInputConnection {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        // The reader blocks in read(); it observes `running` after the next
        // packet or when the node goes away on unplug. Detach rather than
        // join so teardown never blocks on an idle controller.
        self.handle.take();
    }
}

/// Start reading UMP packets from `id`'s endpoint into the event queue.
pub fn open_alsa_ump_input(
    id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
) -> Result<AlsaUmpInputConnection, String> {
    use std::sync::atomic::AtomicBool;

    let node = list_alsa_ump_inputs()
        .into_iter()
        .find(|info| info.id == id)
        .map(|info| info.node)
        .ok_or_else(|| format!("ALSA UMP input {id:?} not found"))?;

    let file = std::fs::File::open(&node).map_err(|e| format!("opening {node}: {e}"))?;

    let running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&running);
    let handle = std::thread::Builder::new()
        .name("vibelang-alsa-ump".into())
        .spawn(move || {
            read_loop(file, id, event_sender, midi_clock, thread_running);
        })
        .map_err(|e| format!("spawning UMP reader for {node}: {e}"))?;

    Ok(AlsaUmpInputConnection {
        running,
        handle: Some(handle),
    })
}

fn read_loop(
    mut file: std::fs::File,
    device_id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
    running: Arc<std::sync::atomic::AtomicBool>,
) {
    use std::io::Read;
    use std::sync::atomic::Ordering;

    let parser = UmpParser::new();
    let origin = std::time::Instant::now();
    let mut buf = [0u8; 1024];
    // Carry for a packet split across reads.
    let mut pending: Vec<u32> = Vec::new();

    while running.load(Ordering::Relaxed) {
        let read = match file.read(&mut buf) {
            Ok(0) => break, // endpoint went away (unplug)
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::debug!("ALSA UMP read error on {device_id:?}: {e}");
                break;
            }
        };
        // The UMP rawmidi node carries no hardware timestamp, so stamp on
        // arrival from the same monotonic origin for every packet.
        let timestamp_us = origin.elapsed().as_micros() as u64;
        midi_clock.calibrate(timestamp_us);

        // UMP is a stream of native-endian 32-bit words.
        for chunk in buf[..read].chunks_exact(4) {
            pending.push(u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        drain_packets(
            &mut pending,
            &parser,
            device_id,
            timestamp_us,
            &event_sender,
        );
    }
}

/// Consume whole UMP packets from `words`, leaving any partial tail.
fn drain_packets(
    words: &mut Vec<u32>,
    parser: &UmpParser,
    device_id: MidiDeviceId,
    timestamp_us: u64,
    event_sender: &MidiEventSender,
) {
    let mut offset = 0;
    while offset < words.len() {
        let packet_words = UmpParser::packet_size(words[offset]);
        let end = offset.saturating_add(packet_words);
        if end > words.len() {
            break; // wait for the rest of this packet
        }
        if let Some(message) = parser.parse(&words[offset..end]) {
            event_sender.try_send(TimestampedMidiEvent::new(
                timestamp_us,
                std::time::Instant::now(),
                device_id,
                message,
            ));
        }
        offset = end;
    }
    words.drain(..offset);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::MidiEventQueue;

    #[test]
    fn node_names_parse() {
        assert_eq!(parse_node_name("umpC1D0"), Some((1, 0)));
        assert_eq!(parse_node_name("umpC12D3"), Some((12, 3)));
        assert_eq!(parse_node_name("midiC1D0"), None);
        assert_eq!(parse_node_name("umpC1"), None);
    }

    #[test]
    fn ids_are_stable_flagged_and_distinct() {
        let a = alsa_ump_input_id("/dev/snd/umpC1D0");
        let b = alsa_ump_input_id("/dev/snd/umpC1D0");
        let c = alsa_ump_input_id("/dev/snd/umpC2D0");
        assert_eq!(a, b, "same node yields the same id");
        assert_ne!(a, c);
        assert!(is_alsa_ump_input_id(a));
        // Must not be mistaken for the PipeWire transport.
        assert!(!super::super::is_pipewire_midi_input_id(a));
    }

    #[test]
    fn drains_whole_packets_and_keeps_partial_tail() {
        let queue = MidiEventQueue::new(64);
        let sender = queue.sender();
        let parser = UmpParser::new();
        let id = alsa_ump_input_id("/dev/snd/umpC1D0");

        // MIDI 2.0 Channel Voice (message type 4) is a 2-word packet:
        // note-on, group 0, channel 0, note 60, velocity 0x8000.
        let word0 = 0x4090_3C00u32;
        let word1 = 0x8000_0000u32;
        assert_eq!(UmpParser::packet_size(word0), 2);

        // A whole packet plus the first word of the next one.
        let mut words = vec![word0, word1, word0];
        drain_packets(&mut words, &parser, id, 0, &sender);
        assert_eq!(words, vec![word0], "partial packet retained for next read");

        // Completing it drains the remainder.
        words.push(word1);
        drain_packets(&mut words, &parser, id, 0, &sender);
        assert!(words.is_empty());
    }
}
