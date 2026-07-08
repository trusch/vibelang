//! PipeWire raw UMP MIDI 2.0 input support.

use crate::midi::{MidiClock, MidiEventSender, TimestampedMidiEvent, UmpParser};
use crate::types::ids::MidiDeviceId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub const PIPEWIRE_MIDI_INPUT_FLAG: u32 = 0x8000_0000;

const SPA_CONTROL_MIDI: u32 = 2;
const SPA_CONTROL_UMP: u32 = 4;
const SPA_TYPE_SEQUENCE: u32 = 14;
const SPA_TYPE_BYTES: u32 = 5;

#[derive(Clone, Debug)]
pub struct PipeWireMidiInputInfo {
    pub id: MidiDeviceId,
    pub name: String,
    pub target_object: String,
}

fn target_cache() -> &'static Mutex<HashMap<MidiDeviceId, String>> {
    static CACHE: OnceLock<Mutex<HashMap<MidiDeviceId, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn is_pipewire_midi_input_id(id: MidiDeviceId) -> bool {
    id.raw() & PIPEWIRE_MIDI_INPUT_FLAG != 0
}

pub fn pipewire_midi_input_id(target_object: &str) -> MidiDeviceId {
    let mut hash = 0x811C_9DC5u32;
    for byte in target_object.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    MidiDeviceId::new(PIPEWIRE_MIDI_INPUT_FLAG | (hash & !PIPEWIRE_MIDI_INPUT_FLAG))
}

fn remember_target(id: MidiDeviceId, target_object: &str) {
    if let Ok(mut cache) = target_cache().lock() {
        cache.insert(id, target_object.to_string());
    }
}

fn cached_target(id: MidiDeviceId) -> Option<String> {
    target_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&id).cloned())
}

/// Decide whether a PipeWire node — described by its **registry-global**
/// props — is a UMP MIDI 2.0 source vibelang can open.
///
/// PipeWire only promotes a small key set to the registry global dict, so
/// `format.dsp` is usually *absent* here even for genuine UMP sources: it
/// lives in the node's info props, which would require a per-node bind to
/// read. We therefore key on `media.class`. Hardware, ALSA-seq and Bluetooth
/// MIDI all surface as `Midi/Bridge`; application-created MIDI streams — UMP
/// in modern PipeWire, e.g. the udp-midi-bridge fake devices — surface as
/// `Midi/Source`. Accept a `Midi/Source` unless it *explicitly* advertises a
/// legacy non-UMP raw-midi format (which, when present, we honour).
#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
fn is_ump_midi_source(media_type: &str, media_class: &str, format_dsp: &str) -> bool {
    if media_type != "Midi" || media_class != "Midi/Source" {
        return false;
    }
    !format_dsp.eq_ignore_ascii_case("8 bit raw midi")
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
fn info_from_props(id: u32, props: &libspa::utils::dict::DictRef) -> Option<PipeWireMidiInputInfo> {
    let media_type = props.get("media.type")?;
    let media_class = props.get("media.class")?;
    let format_dsp = props.get("format.dsp").unwrap_or_default();

    if !is_ump_midi_source(media_type, media_class, format_dsp) {
        return None;
    }

    let node_name = props.get("node.name")?.to_string();
    let description = props
        .get("node.description")
        .unwrap_or(&node_name)
        .to_string();

    let target_object = if node_name.is_empty() {
        id.to_string()
    } else {
        node_name
    };
    let device_id = pipewire_midi_input_id(&target_object);
    remember_target(device_id, &target_object);

    Some(PipeWireMidiInputInfo {
        id: device_id,
        name: description,
        target_object,
    })
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
pub fn list_pipewire_midi2_inputs() -> Vec<PipeWireMidiInputInfo> {
    use pipewire as pw;
    use std::cell::Cell;
    use std::rc::Rc;

    let mainloop = match pw::main_loop::MainLoopRc::new(None) {
        Ok(mainloop) => mainloop,
        Err(e) => {
            tracing::debug!("PipeWire MIDI2 discovery unavailable: {}", e);
            return Vec::new();
        }
    };
    let context = match pw::context::ContextRc::new(&mainloop, None) {
        Ok(context) => context,
        Err(e) => {
            tracing::debug!("PipeWire MIDI2 discovery context failed: {}", e);
            return Vec::new();
        }
    };
    let core = match context.connect_rc(None) {
        Ok(core) => core,
        Err(e) => {
            tracing::debug!("PipeWire MIDI2 discovery connect failed: {}", e);
            return Vec::new();
        }
    };
    let registry = match core.get_registry() {
        Ok(registry) => registry,
        Err(e) => {
            tracing::debug!("PipeWire MIDI2 discovery registry failed: {}", e);
            return Vec::new();
        }
    };

    let devices = Rc::new(std::cell::RefCell::new(Vec::new()));
    let devices_for_listener = Rc::clone(&devices);
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }

            let Some(props) = global.props else {
                return;
            };

            if let Some(info) = info_from_props(global.id, props) {
                devices_for_listener.borrow_mut().push(info);
            }
        })
        .register();

    let done = Rc::new(Cell::new(false));
    let done_for_listener = Rc::clone(&done);
    let loop_for_listener = mainloop.clone();
    let pending = match core.sync(0) {
        Ok(pending) => pending,
        Err(e) => {
            tracing::debug!("PipeWire MIDI2 discovery sync failed: {}", e);
            return Vec::new();
        }
    };
    let _core_listener = core
        .add_listener_local()
        .done(move |id, seq| {
            if id == pw::core::PW_ID_CORE && seq == pending {
                done_for_listener.set(true);
                loop_for_listener.quit();
            }
        })
        .register();

    while !done.get() {
        mainloop.run();
    }

    let mut result = devices.borrow().clone();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result.dedup_by(|a, b| a.id == b.id);
    result
}

#[cfg(not(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32"))))]
pub fn list_pipewire_midi2_inputs() -> Vec<PipeWireMidiInputInfo> {
    Vec::new()
}

/// Snapshot of the `MidiDeviceId`s of every PipeWire MIDI2 input currently
/// present on the system. Used by the hot-plug watcher to detect devices
/// that (re)appear or vanish. Blocking (spins a throwaway PipeWire main
/// loop), so call it off the runtime task.
pub fn pipewire_midi2_input_ids() -> std::collections::HashSet<MidiDeviceId> {
    list_pipewire_midi2_inputs()
        .into_iter()
        .map(|info| info.id)
        .collect()
}

pub struct PipeWireMidiInputConnection {
    #[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
    running: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
impl PipeWireMidiInputConnection {
    fn new(
        running: Arc<std::sync::atomic::AtomicBool>,
        handle: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            running,
            handle: Some(handle),
        }
    }
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
impl Drop for PipeWireMidiInputConnection {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
pub fn open_pipewire_midi2_input(
    id: MidiDeviceId,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
) -> Result<PipeWireMidiInputConnection, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    let target_object = cached_target(id).or_else(|| {
        list_pipewire_midi2_inputs()
            .into_iter()
            .find(|info| info.id == id)
            .map(|info| info.target_object)
    });
    let target_object =
        target_object.ok_or_else(|| format!("PipeWire MIDI2 input {:?} not found", id))?;

    let running = Arc::new(AtomicBool::new(true));
    let running_for_thread = Arc::clone(&running);
    let (ready_tx, ready_rx) = mpsc::channel();
    let thread_name = format!("vibelang-pw-midi2-{}", id.raw());
    let handle = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let setup = run_pipewire_input_loop(
                id,
                target_object,
                event_sender,
                midi_clock,
                Arc::clone(&running_for_thread),
                ready_tx,
            );
            if let Err(e) = setup {
                tracing::warn!("PipeWire MIDI2 input stopped: {}", e);
            }
            running_for_thread.store(false, Ordering::Relaxed);
        })
        .map_err(|e| format!("Failed to spawn PipeWire MIDI2 input thread: {}", e))?;

    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => Ok(PipeWireMidiInputConnection::new(running, handle)),
        Ok(Err(e)) => {
            running.store(false, Ordering::Relaxed);
            let _ = handle.join();
            Err(e)
        }
        Err(e) => {
            running.store(false, Ordering::Relaxed);
            let _ = handle.join();
            Err(format!("Timed out opening PipeWire MIDI2 input: {}", e))
        }
    }
}

#[cfg(not(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32"))))]
pub fn open_pipewire_midi2_input(
    _id: MidiDeviceId,
    _event_sender: MidiEventSender,
    _midi_clock: Arc<MidiClock>,
) -> Result<PipeWireMidiInputConnection, String> {
    Err("PipeWire MIDI2 input is not available on this platform".to_string())
}

#[cfg(all(feature = "pipewire-midi2", unix, not(target_arch = "wasm32")))]
fn run_pipewire_input_loop(
    device_id: MidiDeviceId,
    target_object: String,
    event_sender: MidiEventSender,
    midi_clock: Arc<MidiClock>,
    running: Arc<std::sync::atomic::AtomicBool>,
    ready_tx: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    use pipewire as pw;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let mainloop =
        pw::main_loop::MainLoopBox::new(None).map_err(|e| format!("main loop: {}", e))?;
    let context = pw::context::ContextBox::new(mainloop.loop_(), None)
        .map_err(|e| format!("context: {}", e))?;
    let core = context
        .connect(None)
        .map_err(|e| format!("connect: {}", e))?;

    let node_name = format!("vibelang-midi2-in-{:08x}", device_id.raw());
    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Midi",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_CLASS => "Midi/Sink",
        *pw::keys::NODE_NAME => node_name.as_str(),
        *pw::keys::NODE_DESCRIPTION => "VibeLang MIDI 2.0 Input",
        *pw::keys::FORMAT_DSP => "32 bit raw UMP",
        "target.object" => target_object.as_str(),
    };

    let stream = pw::stream::StreamBox::new(&core, "midi-in", props)
        .map_err(|e| format!("stream: {}", e))?;

    struct StreamData {
        device_id: MidiDeviceId,
        event_sender: MidiEventSender,
        midi_clock: Arc<MidiClock>,
        parser: UmpParser,
    }

    let _listener = stream
        .add_local_listener_with_user_data(StreamData {
            device_id,
            event_sender,
            midi_clock,
            parser: UmpParser::new(),
        })
        .process(|stream, data| {
            if let Some(mut buffer) = stream.dequeue_buffer() {
                let datas = buffer.datas_mut();
                if datas.is_empty() {
                    return;
                }

                let data_buf = &mut datas[0];
                let chunk = data_buf.chunk();
                let size = chunk.size() as usize;
                let offset = chunk.offset() as usize;

                if let Some(slice) = data_buf.data() {
                    if size == 0 || offset + size > slice.len() {
                        return;
                    }
                    parse_pipewire_midi_pod(
                        &slice[offset..offset + size],
                        data.device_id,
                        &data.event_sender,
                        &data.midi_clock,
                        &data.parser,
                    );
                }
            }
        })
        .register();

    stream
        .connect(
            libspa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [],
        )
        .map_err(|e| format!("stream connect: {}", e))?;

    let _ = ready_tx.send(Ok(()));

    while running.load(Ordering::Relaxed) {
        mainloop.loop_().iterate(Duration::from_millis(50));
    }

    tracing::info!(
        "Closed PipeWire MIDI2 input target '{}' ({:?})",
        target_object,
        device_id
    );
    Ok(())
}

pub fn parse_pipewire_midi_pod(
    data: &[u8],
    device_id: MidiDeviceId,
    event_sender: &MidiEventSender,
    midi_clock: &MidiClock,
    parser: &UmpParser,
) {
    if data.len() < 16 {
        parse_ump_bytes(data, 0, device_id, event_sender, midi_clock, parser);
        return;
    }

    let pod_size = read_u32_ne(data, 0).unwrap_or_default() as usize;
    let pod_type = read_u32_ne(data, 4).unwrap_or_default();

    if pod_type != SPA_TYPE_SEQUENCE {
        parse_ump_bytes(data, 0, device_id, event_sender, midi_clock, parser);
        return;
    }

    let mut offset = 16;
    let end = (8 + pod_size).min(data.len());

    while offset + 16 <= end {
        let control_offset = read_u32_ne(data, offset).unwrap_or_default() as u64;
        let control_type = read_u32_ne(data, offset + 4).unwrap_or_default();
        let value_size = read_u32_ne(data, offset + 8).unwrap_or_default() as usize;
        let value_type = read_u32_ne(data, offset + 12).unwrap_or_default();
        let value_start = offset + 16;
        let value_end = value_start.saturating_add(value_size);

        if value_type == SPA_TYPE_BYTES && value_end <= end {
            let value_data = &data[value_start..value_end];
            match control_type {
                SPA_CONTROL_UMP => {
                    parse_ump_bytes(
                        value_data,
                        control_offset,
                        device_id,
                        event_sender,
                        midi_clock,
                        parser,
                    );
                }
                SPA_CONTROL_MIDI => {
                    if let Some(msg) = crate::midi::parse_midi_bytes(value_data) {
                        midi_clock.calibrate(control_offset);
                        event_sender.try_send(TimestampedMidiEvent::new(
                            control_offset,
                            std::time::Instant::now(),
                            device_id,
                            msg,
                        ));
                    }
                }
                _ => {}
            }
        }

        let control_size = 8 + 8 + value_size;
        offset += (control_size + 7) & !7;
    }
}

fn parse_ump_bytes(
    data: &[u8],
    timestamp_us: u64,
    device_id: MidiDeviceId,
    event_sender: &MidiEventSender,
    midi_clock: &MidiClock,
    parser: &UmpParser,
) {
    if data.len() < 4 {
        return;
    }

    let mut words = Vec::with_capacity(data.len() / 4);
    for chunk in data.chunks_exact(4) {
        words.push(u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    let mut offset = 0;
    while offset < words.len() {
        let packet_words = UmpParser::packet_size(words[offset]);
        let end = offset.saturating_add(packet_words);
        if end > words.len() {
            break;
        }

        if let Some(message) = parser.parse(&words[offset..end]) {
            midi_clock.calibrate(timestamp_us);
            event_sender.try_send(TimestampedMidiEvent::new(
                timestamp_us,
                std::time::Instant::now(),
                device_id,
                message,
            ));
        }

        offset = end;
    }
}

fn read_u32_ne(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi::{Channel, MidiEventQueue, MidiMessage};

    fn pod_sequence_with_control(control_type: u32, value: &[u8]) -> Vec<u8> {
        let aligned_value_len = (value.len() + 7) & !7;
        let pod_size = 8 + 8 + 8 + aligned_value_len;
        let mut data = Vec::with_capacity(8 + pod_size);
        data.extend_from_slice(&(pod_size as u32).to_ne_bytes());
        data.extend_from_slice(&SPA_TYPE_SEQUENCE.to_ne_bytes());
        data.extend_from_slice(&0u32.to_ne_bytes());
        data.extend_from_slice(&0u32.to_ne_bytes());
        data.extend_from_slice(&0u32.to_ne_bytes());
        data.extend_from_slice(&control_type.to_ne_bytes());
        data.extend_from_slice(&(value.len() as u32).to_ne_bytes());
        data.extend_from_slice(&SPA_TYPE_BYTES.to_ne_bytes());
        data.extend_from_slice(value);
        data.resize(8 + pod_size, 0);
        data
    }

    #[test]
    fn ump_source_detected_without_format_dsp_in_global_props() {
        // The udp-midi-bridge fake device: registry-global props carry no
        // format.dsp and the name has no "ump" substring — must still match.
        assert!(is_ump_midi_source("Midi", "Midi/Source", ""));
    }

    #[test]
    fn ump_source_detected_with_explicit_ump_format() {
        assert!(is_ump_midi_source("Midi", "Midi/Source", "32 bit raw UMP"));
    }

    #[test]
    fn midi_bridge_is_not_a_ump_source() {
        // Hardware / ALSA-seq / Bluetooth MIDI surfaces as Midi/Bridge.
        assert!(!is_ump_midi_source("Midi", "Midi/Bridge", ""));
    }

    #[test]
    fn non_midi_node_is_not_a_ump_source() {
        assert!(!is_ump_midi_source("Audio", "Audio/Source", ""));
    }

    #[test]
    fn legacy_raw_midi_source_is_rejected() {
        // A node that explicitly declares the legacy MIDI 1.0 raw format.
        assert!(!is_ump_midi_source("Midi", "Midi/Source", "8 bit raw midi"));
    }

    #[test]
    fn pipewire_ids_use_reserved_namespace() {
        let id = pipewire_midi_input_id("MPK-FAKE");

        assert!(is_pipewire_midi_input_id(id));
        assert_eq!(id, pipewire_midi_input_id("MPK-FAKE"));
    }

    #[test]
    fn parses_pipewire_ump_sequence_into_event_queue() {
        let queue = MidiEventQueue::with_default_capacity();
        let parser = UmpParser::new();
        let device_id = pipewire_midi_input_id("MPK-FAKE");
        let word = 0x20903C64u32;
        let pod = pod_sequence_with_control(SPA_CONTROL_UMP, &word.to_ne_bytes());

        parse_pipewire_midi_pod(
            &pod,
            device_id,
            &queue.sender(),
            &MidiClock::default(),
            &parser,
        );

        let events: Vec<_> = queue.drain().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].device_id, device_id);
        match &events[0].message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => {
                assert_eq!(*channel, Channel::new(0));
                assert_eq!(*note, 60);
                assert_eq!(velocity.to_midi1(), 100);
            }
            other => panic!("expected NoteOn, got {other:?}"),
        }
    }

    #[test]
    fn parses_pipewire_legacy_midi_sequence_into_event_queue() {
        let queue = MidiEventQueue::with_default_capacity();
        let parser = UmpParser::new();
        let device_id = pipewire_midi_input_id("MPK-FAKE");
        let pod = pod_sequence_with_control(SPA_CONTROL_MIDI, &[0x90, 60, 100]);

        parse_pipewire_midi_pod(
            &pod,
            device_id,
            &queue.sender(),
            &MidiClock::default(),
            &parser,
        );

        let events: Vec<_> = queue.drain().collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].message,
            MidiMessage::NoteOn {
                channel,
                note: 60,
                velocity,
            } if *channel == Channel::new(0) && velocity.to_midi1() == 100
        ));
    }
}
