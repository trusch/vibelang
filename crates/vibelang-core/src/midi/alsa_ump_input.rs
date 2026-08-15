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
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Marks a [`MidiDeviceId`] as an ALSA UMP endpoint.
pub const ALSA_UMP_INPUT_FLAG: u32 = 0x4000_0000;

#[derive(Clone, Debug)]
pub struct AlsaUmpInputInfo {
    pub id: MidiDeviceId,
    /// Human-readable name, e.g. `"Gamma (UMP)"`.
    pub name: String,
    /// Device node path, e.g. `/dev/snd/umpC1D0`.
    pub node: String,
}

pub fn is_alsa_ump_input_id(id: MidiDeviceId) -> bool {
    id.raw() & super::MIDI_INPUT_TRANSPORT_MASK == ALSA_UMP_INPUT_FLAG
}

/// Derive a stable id from the device node name (FNV-1a, flagged).
pub fn alsa_ump_input_id(node: &str) -> MidiDeviceId {
    let mut hash = 0x811C_9DC5u32;
    for byte in node.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    MidiDeviceId::new(ALSA_UMP_INPUT_FLAG | (hash & !super::MIDI_INPUT_TRANSPORT_MASK))
}

/// `"umpC1D0"` -> `Some((1, 0))`.
fn parse_node_name(name: &str) -> Option<(u32, u32)> {
    let rest = name.strip_prefix("umpC")?;
    let (card, device) = rest.split_once('D')?;
    Some((card.parse().ok()?, device.parse().ok()?))
}

fn discover_ump_nodes() -> Vec<(u32, u32, String)> {
    std::fs::read_dir("/dev/snd")
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let (card, device) = parse_node_name(&file_name)?;
            Some((card, device, format!("/dev/snd/{file_name}")))
        })
        .collect()
}

/// Card name from `/proc/asound/card<N>/id`, falling back to the node name.
fn card_name(card: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/asound/card{card}/id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[repr(C, packed)]
struct UmpEndpointInfo {
    card: i32,
    device: i32,
    flags: u32,
    protocol_caps: u32,
    protocol: u32,
    num_blocks: u32,
    version: u16,
    family_id: u16,
    model_id: u16,
    manufacturer_id: u32,
    sw_revision: [u8; 4],
    padding: u16,
    name: [u8; 128],
    product_id: [u8; 128],
    reserved: [u8; 32],
}

impl Default for UmpEndpointInfo {
    fn default() -> Self {
        Self {
            card: 0,
            device: 0,
            flags: 0,
            protocol_caps: 0,
            protocol: 0,
            num_blocks: 0,
            version: 0,
            family_id: 0,
            model_id: 0,
            manufacturer_id: 0,
            sw_revision: [0; 4],
            padding: 0,
            name: [0; 128],
            product_id: [0; 128],
            reserved: [0; 32],
        }
    }
}

#[repr(C, packed)]
struct UmpBlockInfo {
    card: i32,
    device: i32,
    block_id: u8,
    direction: u8,
    active: u8,
    first_group: u8,
    num_groups: u8,
    midi_ci_version: u8,
    sysex8_streams: u8,
    ui_hint: u8,
    flags: u32,
    name: [u8; 128],
    reserved: [u8; 32],
}

impl Default for UmpBlockInfo {
    fn default() -> Self {
        Self {
            card: 0,
            device: 0,
            block_id: 0,
            direction: 0,
            active: 0,
            first_group: 0,
            num_groups: 0,
            midi_ci_version: 0,
            sysex8_streams: 0,
            ui_hint: 0,
            flags: 0,
            name: [0; 128],
            reserved: [0; 32],
        }
    }
}

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn linux_iowr<T>(kind: u8, number: u8) -> libc::c_ulong {
    (((IOC_READ | IOC_WRITE) << IOC_DIRSHIFT)
        | ((kind as u32) << IOC_TYPESHIFT)
        | ((number as u32) << IOC_NRSHIFT)
        | ((std::mem::size_of::<T>() as u32) << IOC_SIZESHIFT)) as libc::c_ulong
}

const CTL_UMP_ENDPOINT_INFO: libc::c_ulong = linux_iowr::<UmpEndpointInfo>(b'U', 0x44);
const CTL_UMP_BLOCK_INFO: libc::c_ulong = linux_iowr::<UmpBlockInfo>(b'U', 0x45);
const UMP_DIRECTION_INPUT: u8 = 0x01;
const MAX_UMP_BLOCKS: u32 = 32;

fn endpoint_has_input(card: u32, device: u32) -> bool {
    match query_endpoint_has_input(card, device) {
        Ok(has_input) => has_input,
        Err(e) => {
            tracing::debug!("Unable to inspect ALSA UMP endpoint {card}:{device} direction: {e}");
            false
        }
    }
}

fn query_endpoint_has_input(card: u32, device: u32) -> io::Result<bool> {
    let control = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(format!("/dev/snd/controlC{card}"))?;

    let mut endpoint = UmpEndpointInfo {
        card: card as i32,
        device: device as i32,
        ..UmpEndpointInfo::default()
    };
    let result = unsafe {
        libc::ioctl(
            control.as_raw_fd(),
            CTL_UMP_ENDPOINT_INFO,
            &mut endpoint as *mut UmpEndpointInfo,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    let num_blocks = unsafe { std::ptr::addr_of!(endpoint.num_blocks).read_unaligned() };
    for block_id in 0..num_blocks.min(MAX_UMP_BLOCKS) {
        let mut block = UmpBlockInfo {
            card: card as i32,
            device: device as i32,
            block_id: block_id as u8,
            ..UmpBlockInfo::default()
        };
        let result = unsafe {
            libc::ioctl(
                control.as_raw_fd(),
                CTL_UMP_BLOCK_INFO,
                &mut block as *mut UmpBlockInfo,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        if block.direction & UMP_DIRECTION_INPUT != 0 {
            return Ok(true);
        }
    }

    Ok(false)
}

fn build_input_infos<I, D, N>(
    nodes: I,
    mut has_input: D,
    mut name_for_card: N,
) -> Vec<AlsaUmpInputInfo>
where
    I: IntoIterator<Item = (u32, u32, String)>,
    D: FnMut(u32, u32) -> bool,
    N: FnMut(u32) -> Option<String>,
{
    let mut found: Vec<_> = nodes
        .into_iter()
        .filter(|(card, device, _)| has_input(*card, *device))
        .map(|(card, device, node)| {
            let name = match name_for_card(card) {
                Some(card_id) => format!("{card_id} (UMP)"),
                None => format!("UMP {card}:{device}"),
            };
            AlsaUmpInputInfo {
                id: alsa_ump_input_id(&node),
                name,
                node,
            }
        })
        .collect();
    found.sort_by(|a, b| a.node.cmp(&b.node));
    found
}

/// Enumerate input-capable UMP endpoints exposed by the kernel.
pub fn list_alsa_ump_inputs() -> Vec<AlsaUmpInputInfo> {
    build_input_infos(discover_ump_nodes(), endpoint_has_input, card_name)
}

enum ReaderResult {
    Data(usize),
    Closed,
    Cancelled,
}

trait UmpReader: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<ReaderResult>;
}

trait ReaderWake: Send + Sync {
    fn wake(&self);
}

struct PollFileReader {
    file: File,
    wake_read: UnixStream,
}

impl UmpReader for PollFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<ReaderResult> {
        loop {
            let mut fds = [
                libc::pollfd {
                    fd: self.file.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: self.wake_read.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }

            if fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
            {
                return Ok(ReaderResult::Cancelled);
            }
            if fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) == 0
            {
                continue;
            }

            match self.file.read(buf) {
                Ok(0) => return Ok(ReaderResult::Closed),
                Ok(read) => return Ok(ReaderResult::Data(read)),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(e),
            }
        }
    }
}

struct SocketWake(UnixStream);

impl ReaderWake for SocketWake {
    fn wake(&self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

/// Open handle; dropping it cancels, wakes, closes, and joins the reader.
pub struct AlsaUmpInputConnection {
    running: Arc<AtomicBool>,
    wake: Arc<dyn ReaderWake>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AlsaUmpInputConnection {
    pub fn is_alive(&self) -> bool {
        self.running.load(Ordering::Acquire)
            && self
                .handle
                .as_ref()
                .map(|handle| !handle.is_finished())
                .unwrap_or(false)
    }

    fn shutdown(&mut self) {
        self.running.store(false, Ordering::Release);
        self.wake.wake();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AlsaUmpInputConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn open_nonblocking_endpoint_with<T, F>(node: &str, open: F) -> Result<T, String>
where
    F: FnOnce(&str, i32) -> io::Result<T>,
{
    open(node, libc::O_NONBLOCK).map_err(|e| match e.raw_os_error() {
        Some(libc::EAGAIN) | Some(libc::EBUSY) => format!("ALSA UMP input {node} is busy"),
        _ => format!("opening {node}: {e}"),
    })
}

fn open_nonblocking_endpoint(node: &str) -> Result<File, String> {
    open_nonblocking_endpoint_with(node, |path, flags| {
        OpenOptions::new().read(true).custom_flags(flags).open(path)
    })
}

/// Start reading UMP packets from `id`'s endpoint into the event queue.
pub fn open_alsa_ump_input(
    id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
) -> Result<AlsaUmpInputConnection, String> {
    let node = discover_ump_nodes()
        .into_iter()
        .map(|(_, _, node)| node)
        .find(|node| alsa_ump_input_id(node) == id)
        .ok_or_else(|| format!("ALSA UMP input {id:?} not found"))?;

    let file = open_nonblocking_endpoint(&node)?;
    let (wake_read, wake_write) =
        UnixStream::pair().map_err(|e| format!("creating reader wake channel for {node}: {e}"))?;
    spawn_alsa_ump_reader(
        Box::new(PollFileReader { file, wake_read }),
        Arc::new(SocketWake(wake_write)),
        id,
        event_sender,
        midi_clock,
    )
    .map_err(|e| format!("spawning UMP reader for {node}: {e}"))
}

fn spawn_alsa_ump_reader(
    reader: Box<dyn UmpReader>,
    wake: Arc<dyn ReaderWake>,
    id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
) -> io::Result<AlsaUmpInputConnection> {
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&running);
    let handle = std::thread::Builder::new()
        .name("vibelang-alsa-ump".into())
        .spawn(move || {
            read_loop(
                reader,
                id,
                event_sender,
                midi_clock,
                Arc::clone(&thread_running),
            );
            thread_running.store(false, Ordering::Release);
        })?;

    Ok(AlsaUmpInputConnection {
        running,
        wake,
        handle: Some(handle),
    })
}

fn read_loop(
    mut reader: Box<dyn UmpReader>,
    device_id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
    running: Arc<AtomicBool>,
) {
    let parser = UmpParser::new();
    let origin = std::time::Instant::now();
    let mut buf = [0u8; 1024];
    let mut pending: Vec<u32> = Vec::new();

    while running.load(Ordering::Acquire) {
        let read = match reader.read(&mut buf) {
            Ok(ReaderResult::Data(read)) => read,
            Ok(ReaderResult::Closed | ReaderResult::Cancelled) => break,
            Err(e) => {
                tracing::debug!("ALSA UMP read error on {device_id:?}: {e}");
                break;
            }
        };
        if !running.load(Ordering::Acquire) {
            break;
        }

        let timestamp_us = origin.elapsed().as_micros() as u64;
        midi_clock.calibrate(timestamp_us);
        for chunk in buf[..read].chunks_exact(4) {
            pending.push(u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        drain_packets(
            &mut pending,
            &parser,
            device_id,
            timestamp_us,
            &event_sender,
            &running,
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
    running: &AtomicBool,
) {
    let mut offset = 0;
    while offset < words.len() && running.load(Ordering::Acquire) {
        let packet_words = UmpParser::packet_size(words[offset]);
        let end = offset.saturating_add(packet_words);
        if end > words.len() {
            break;
        }
        if let Some(message) = parser.parse(&words[offset..end]) {
            if !running.load(Ordering::Acquire) {
                break;
            }
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
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    #[test]
    fn node_names_parse() {
        assert_eq!(parse_node_name("umpC1D0"), Some((1, 0)));
        assert_eq!(parse_node_name("umpC12D3"), Some((12, 3)));
        assert_eq!(parse_node_name("midiC1D0"), None);
        assert_eq!(parse_node_name("umpC1"), None);
    }

    #[test]
    fn ids_are_stable_and_exactly_classified() {
        let a = alsa_ump_input_id("/dev/snd/umpC1D0");
        let b = alsa_ump_input_id("/dev/snd/umpC1D0");
        let c = alsa_ump_input_id("/dev/snd/umpC2D0");
        assert_eq!(a, b, "same node yields the same id");
        assert_ne!(a, c);
        assert!(is_alsa_ump_input_id(a));
        assert!(!is_alsa_ump_input_id(MidiDeviceId::new(0xC000_0000)));
        assert!(!super::super::is_pipewire_midi_input_id(a));
    }

    #[test]
    fn direction_aware_enumeration_excludes_output_only_endpoints() {
        let nodes = vec![
            (1, 0, "/dev/snd/umpC1D0".to_string()),
            (1, 1, "/dev/snd/umpC1D1".to_string()),
            (2, 0, "/dev/snd/umpC2D0".to_string()),
        ];
        let inputs = build_input_infos(
            nodes,
            |card, device| matches!((card, device), (1, 0) | (2, 0)),
            |card| Some(format!("card-{card}")),
        );

        assert_eq!(
            inputs
                .iter()
                .map(|info| info.node.as_str())
                .collect::<Vec<_>>(),
            vec!["/dev/snd/umpC1D0", "/dev/snd/umpC2D0"]
        );
    }

    #[test]
    fn kernel_ump_ioctl_struct_layout_matches_uapi() {
        assert_eq!(std::mem::size_of::<UmpEndpointInfo>(), 328);
        assert_eq!(std::mem::size_of::<UmpBlockInfo>(), 180);
    }

    #[test]
    fn busy_open_is_nonblocking_and_returns_immediately() {
        let mut calls = 0;
        let error = open_nonblocking_endpoint_with::<(), _>("/dev/snd/umpC1D0", |_, flags| {
            calls += 1;
            assert_ne!(flags & libc::O_NONBLOCK, 0);
            Err(io::Error::from_raw_os_error(libc::EBUSY))
        })
        .unwrap_err();

        assert_eq!(calls, 1);
        assert!(error.contains("is busy"));
    }

    fn running() -> AtomicBool {
        AtomicBool::new(true)
    }

    #[test]
    fn frames_every_word_count_class_and_split_stream_packet() {
        let parser = UmpParser::new();
        let id = alsa_ump_input_id("/dev/snd/umpC1D0");

        for message_type in [0x7u32, 0x8, 0xB, 0xF] {
            let queue = MidiEventQueue::new(8);
            let word_count = UmpParser::packet_size(message_type << 28);
            let mut words = vec![message_type << 28];
            words.resize(word_count, 0);
            words.push(0x2090_3C64);
            drain_packets(&mut words, &parser, id, 0, &queue.sender(), &running());
            assert!(words.is_empty(), "message type {message_type:#x}");
            assert_eq!(queue.len(), 1, "message type {message_type:#x}");
        }

        let queue = MidiEventQueue::new(8);
        let mut words = vec![0xF000_0000, 0x1111_1111];
        drain_packets(&mut words, &parser, id, 0, &queue.sender(), &running());
        assert_eq!(words.len(), 2, "split stream packet must be retained");
        words.extend([0x2222_2222, 0x3333_3333, 0x2090_3C64]);
        drain_packets(&mut words, &parser, id, 0, &queue.sender(), &running());
        assert!(words.is_empty());
        assert_eq!(queue.len(), 1, "following supported packet stays aligned");
    }

    enum InjectedRead {
        Data(Vec<u8>),
        Eof,
    }

    #[derive(Default)]
    struct InjectedState {
        reads: VecDeque<InjectedRead>,
        cancelled: bool,
        late_on_cancel: Option<Vec<u8>>,
        wakes: usize,
    }

    #[derive(Clone)]
    struct InjectedControl {
        state: Arc<(Mutex<InjectedState>, Condvar)>,
        closed: Arc<AtomicBool>,
    }

    impl InjectedControl {
        fn data(&self, words: &[u32]) {
            let bytes = words.iter().flat_map(|word| word.to_ne_bytes()).collect();
            let (state, changed) = &*self.state;
            state
                .lock()
                .unwrap()
                .reads
                .push_back(InjectedRead::Data(bytes));
            changed.notify_all();
        }

        fn eof(&self) {
            let (state, changed) = &*self.state;
            state.lock().unwrap().reads.push_back(InjectedRead::Eof);
            changed.notify_all();
        }

        fn wakes(&self) -> usize {
            self.state.0.lock().unwrap().wakes
        }
    }

    struct InjectedReader {
        control: InjectedControl,
    }

    impl Drop for InjectedReader {
        fn drop(&mut self) {
            self.control.closed.store(true, Ordering::Release);
        }
    }

    impl UmpReader for InjectedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<ReaderResult> {
            let (state, changed) = &*self.control.state;
            let mut state = state.lock().unwrap();
            loop {
                if state.cancelled {
                    if let Some(data) = state.late_on_cancel.take() {
                        let read = data.len().min(buf.len());
                        buf[..read].copy_from_slice(&data[..read]);
                        return Ok(ReaderResult::Data(read));
                    }
                    return Ok(ReaderResult::Cancelled);
                }
                match state.reads.pop_front() {
                    Some(InjectedRead::Data(data)) => {
                        let read = data.len().min(buf.len());
                        buf[..read].copy_from_slice(&data[..read]);
                        return Ok(ReaderResult::Data(read));
                    }
                    Some(InjectedRead::Eof) => return Ok(ReaderResult::Closed),
                    None => state = changed.wait(state).unwrap(),
                }
            }
        }
    }

    struct InjectedWake(InjectedControl);

    impl ReaderWake for InjectedWake {
        fn wake(&self) {
            let (state, changed) = &*self.0.state;
            let mut state = state.lock().unwrap();
            state.cancelled = true;
            state.wakes += 1;
            changed.notify_all();
        }
    }

    fn injected_pair(
        late_on_cancel: Option<Vec<u8>>,
    ) -> (Box<dyn UmpReader>, Arc<dyn ReaderWake>, InjectedControl) {
        let control = InjectedControl {
            state: Arc::new((
                Mutex::new(InjectedState {
                    late_on_cancel,
                    ..InjectedState::default()
                }),
                Condvar::new(),
            )),
            closed: Arc::new(AtomicBool::new(false)),
        };
        (
            Box::new(InjectedReader {
                control: control.clone(),
            }),
            Arc::new(InjectedWake(control.clone())),
            control,
        )
    }

    fn wait_until(mut predicate: impl FnMut() -> bool) {
        for _ in 0..100 {
            if predicate() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("condition was not reached");
    }

    fn spawn_injected(
        reader: Box<dyn UmpReader>,
        wake: Arc<dyn ReaderWake>,
        id: MidiDeviceId,
        queue: &MidiEventQueue,
    ) -> AlsaUmpInputConnection {
        spawn_alsa_ump_reader(
            reader,
            wake,
            id,
            queue.sender(),
            Arc::new(MidiClock::default()),
        )
        .unwrap()
    }

    #[test]
    fn injectable_reader_covers_close_reopen_unplug_and_replug() {
        let id = alsa_ump_input_id("/dev/snd/umpC1D0");
        let queue = MidiEventQueue::new(8);
        let note = 0x2090_3C64u32;
        let late = note.to_ne_bytes().to_vec();

        let (reader, wake, idle) = injected_pair(Some(late));
        let connection = spawn_injected(reader, wake, id, &queue);
        drop(connection);
        assert!(idle.closed.load(Ordering::Acquire));
        assert_eq!(idle.wakes(), 1);
        assert!(queue.recv_timeout(Duration::from_millis(10)).is_none());

        let (reader, wake, reopened) = injected_pair(None);
        let connection = spawn_injected(reader, wake, id, &queue);
        reopened.data(&[note]);
        assert!(queue.recv_timeout(Duration::from_secs(1)).is_some());
        drop(connection);
        assert!(reopened.closed.load(Ordering::Acquire));

        let (reader, wake, unplugged) = injected_pair(None);
        let connection = spawn_injected(reader, wake, id, &queue);
        let mut open_inputs = HashMap::from([(id, connection)]);
        unplugged.eof();
        wait_until(|| !open_inputs.get(&id).unwrap().is_alive());
        let stale = open_inputs.remove(&id).unwrap();
        drop(stale);
        assert!(unplugged.closed.load(Ordering::Acquire));

        let (reader, wake, replugged) = injected_pair(None);
        open_inputs.insert(id, spawn_injected(reader, wake, id, &queue));
        replugged.data(&[note]);
        assert!(queue.recv_timeout(Duration::from_secs(1)).is_some());
        drop(open_inputs.remove(&id));
        assert!(replugged.closed.load(Ordering::Acquire));
    }
}
