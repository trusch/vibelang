//! SuperCollider (scsynth) backend implementation.
//!
//! This module provides a [`Backend`] implementation for SuperCollider's
//! synthesis server (scsynth) using OSC over UDP.
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core::backends::ScsynthBackend;
//! use vibelang_core::Runtime;
//!
//! // Connect to scsynth on default port
//! let backend = ScsynthBackend::connect("127.0.0.1:57110").await?;
//! let runtime = Runtime::new(backend);
//! ```

use crate::backend::{AddAction, Backend, BufferInfo};
use crate::types::{BufferId, NodeId, ParamMap};
use async_trait::async_trait;
use rosc::{decoder, encoder, OscBundle, OscMessage, OscPacket, OscTime, OscType};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

/// Error type for scsynth backend operations.
#[derive(Debug)]
pub enum ScsynthError {
    /// IO error during network communication.
    Io(io::Error),
    /// OSC encoding/decoding error.
    Osc(rosc::OscError),
    /// Connection failed.
    ConnectionFailed(String),
    /// Server not ready.
    ServerNotReady,
    /// Timeout waiting for response.
    Timeout,
    /// A mutex lock was poisoned.
    LockPoisoned,
    /// scsynth rejected a synthdef during /d_recv.
    SynthDefRejected {
        name: String,
        reason: String,
        missing_ugen: Option<String>,
    },
}

impl std::fmt::Display for ScsynthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScsynthError::Io(e) => write!(f, "IO error: {}", e),
            ScsynthError::Osc(e) => write!(f, "OSC error: {:?}", e),
            ScsynthError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            ScsynthError::ServerNotReady => write!(f, "Server not ready"),
            ScsynthError::Timeout => write!(f, "Timeout waiting for response"),
            ScsynthError::LockPoisoned => write!(f, "Internal mutex lock poisoned"),
            ScsynthError::SynthDefRejected { name, reason, .. } => {
                write!(f, "SynthDef '{}' rejected by scsynth: {}", name, reason)
            }
        }
    }
}

impl std::error::Error for ScsynthError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScsynthError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ScsynthError {
    fn from(e: io::Error) -> Self {
        ScsynthError::Io(e)
    }
}

impl From<rosc::OscError> for ScsynthError {
    fn from(e: rosc::OscError) -> Self {
        ScsynthError::Osc(e)
    }
}

const SYNTHDEF_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
const D_RECV_MAX_BYTES: usize = 32 * 1024;
static TEMP_SYNTHDEF_COUNTER: AtomicU64 = AtomicU64::new(0);

struct PendingSynthDefLoad {
    name: String,
    command: &'static str,
    tx: oneshot::Sender<SynthDefLoadReply>,
}

enum SynthDefLoadReply {
    Done,
    Fail { reason: String },
}

/// OSC response message from scsynth.
#[derive(Debug, Clone)]
pub enum OscResponse {
    /// Server status response.
    Status {
        num_ugens: i32,
        num_synths: i32,
        num_groups: i32,
        num_synthdefs: i32,
        avg_cpu: f32,
        peak_cpu: f32,
        sample_rate: f64,
        actual_sample_rate: f64,
    },
    /// Node ended (synth finished or was freed).
    NodeEnd {
        node_id: NodeId,
        parent_group: NodeId,
        prev_node: NodeId,
        next_node: NodeId,
        is_group: bool,
        head_node: Option<NodeId>,
        tail_node: Option<NodeId>,
    },
    /// Node started.
    NodeGo {
        node_id: NodeId,
        parent_group: NodeId,
        prev_node: NodeId,
        next_node: NodeId,
        is_group: bool,
        head_node: Option<NodeId>,
        tail_node: Option<NodeId>,
    },
    /// Buffer info response.
    BufferInfo {
        buffer_id: BufferId,
        frames: u32,
        channels: u16,
        sample_rate: f64,
    },
    /// Command completed successfully.
    Done { command: String },
    /// Command failed.
    Fail { command: String, reason: String },
    /// Trigger message (SendTrig from synth).
    /// Used for metering and other real-time feedback.
    Trigger {
        node_id: NodeId,
        trig_id: i32,
        value: f32,
    },
    /// Unknown response.
    Unknown { path: String, args: Vec<OscType> },
}

/// Callback type for OSC responses.
pub type OscCallback = Arc<dyn Fn(OscResponse) + Send + Sync>;

/// A synth scsynth refused to create because its SynthDef is not loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSynthDef {
    /// The SynthDef name the `/s_new` asked for.
    pub def: String,
    /// The node id the synth would have taken.
    pub node: NodeId,
    /// The server's own words.
    pub reason: String,
}

/// One synth from the server's node tree, with its control values.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeSynth {
    /// Node id as the server reports it.
    pub node: i32,
    /// The SynthDef this node was built from.
    pub def: String,
    /// Control values. Controls mapped to a bus report as a symbol on the
    /// wire and are dropped here — a mapped control names no fixed value.
    pub controls: HashMap<String, f32>,
}

/// A live node writing an audio bus that nothing downstream reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPathBreak {
    /// The node whose output goes nowhere.
    pub node: i32,
    /// Its SynthDef name.
    pub def: String,
    /// The bus it writes.
    pub bus: u32,
    /// The last node that read that bus — earlier in the same cycle, which
    /// is why the write is lost.
    pub consumer_node: i32,
    /// That reader's SynthDef name.
    pub consumer_def: String,
}

impl std::fmt::Display for AudioPathBreak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "node {} ({}) writes audio bus {}, but the last node to read bus {} \
             is node {} ({}) EARLIER in the same cycle — everything feeding \
             node {} is silent",
            self.node,
            self.def,
            self.bus,
            self.bus,
            self.consumer_node,
            self.consumer_def,
            self.node,
        )
    }
}

/// The audio-fabric bus a synth reads and the one it writes, if it is one of
/// the three families that carry group audio.
///
/// Deliberately narrow. These three are audio-rate by construction, so their
/// bus numbers are unambiguously audio-bus numbers; a generic "any control
/// named like a bus" rule would collide with the control-bus namespace (a kr
/// port's `out` is a *control* bus 17, which is not audio bus 17) and invent
/// breaks that are not there. Voices need no entry: they are added at the
/// head of their own group, always ahead of the fabric that carries them.
fn audio_fabric_ports(synth: &TreeSynth) -> Option<(u32, u32)> {
    let pair = |read: &str, write: &str| -> Option<(u32, u32)> {
        Some((
            *synth.controls.get(read)? as u32,
            *synth.controls.get(write)? as u32,
        ))
    };
    // `define_fx` inserts: read and rewrite a group bus in place.
    if let Some(ports) = pair("__fx_bus_in", "__fx_bus_out") {
        return Some(ports);
    }
    // A group's link synth: its bus into the parent's bus (or hardware).
    if synth.def.starts_with("system_link_audio") {
        return pair("inbus", "outbus");
    }
    // A voice's output port into a group bus.
    if synth.def.starts_with("port_to_group_link") {
        return pair("in_bus", "out_bus");
    }
    None
}

/// The nodes in `tree` whose audio output is dropped on the floor.
///
/// `tree` must be in evaluation order — the order scsynth walks the node
/// tree, which is the order `/g_queryTree` reports it in.
///
/// The rule: scsynth zeroes audio buses every block, so a write only counts
/// if some node reads that bus LATER in the same cycle. A write whose bus is
/// read only earlier is lost, and the whole branch feeding it is inaudible
/// while every synth in it is alive, correctly parameterised, and built from
/// a SynthDef the server definitely has. That is the shape a reload leaves
/// behind when it places a node on the wrong side of the node that consumes
/// its bus.
///
/// A bus that nothing reads anywhere is a sink — the hardware outputs — and
/// is never reported.
pub fn audio_path_breaks(tree: &[TreeSynth]) -> Vec<AudioPathBreak> {
    let fabric: Vec<(usize, &TreeSynth, u32, u32)> = tree
        .iter()
        .enumerate()
        .filter_map(|(i, s)| audio_fabric_ports(s).map(|(r, w)| (i, s, r, w)))
        .collect();

    fabric
        .iter()
        .filter_map(|(index, synth, _, written)| {
            // An in-place insert reads and writes at the same index; its own
            // read cannot consume its own write, hence the strict `>`.
            let consumed_later = fabric
                .iter()
                .any(|(other, _, read, _)| other > index && read == written);
            if consumed_later {
                return None;
            }
            // Nothing reads this bus at all: a hardware sink, not a break.
            let last_reader = fabric
                .iter()
                .filter(|(other, _, read, _)| other < index && read == written)
                .next_back()?;
            Some(AudioPathBreak {
                node: synth.node,
                def: synth.def.clone(),
                bus: *written,
                consumer_node: last_reader.1.node,
                consumer_def: last_reader.1.def.clone(),
            })
        })
        .collect()
}

/// Which `/s_new` requests the server has answered, and which it refused.
///
/// scsynth accepts a `/d_recv` whose graph it cannot build — it prints
/// `exception in GraphDef_Recv` to its own stdout and still replies
/// `/done`, so a `/d_recv` reply proves nothing about whether the def is
/// usable. The refusal only surfaces later, as `/fail /s_new SynthDef not
/// found` when a voice is instantiated, and that `/fail` carries no node
/// id or def name — so on its own it cannot say WHICH voice went missing.
///
/// Attribution is by order. scsynth executes commands in the order it
/// receives them and answers each one before the next, so when a `/fail
/// /s_new` arrives, the oldest `/s_new` that has not yet come back as
/// `/n_go` is the one that failed.
#[derive(Default)]
struct GraphIntegrity {
    /// `/s_new` requests still waiting for `/n_go` or `/fail`, in send order.
    outstanding: std::collections::VecDeque<(NodeId, String)>,
    /// Refusals observed since the last take.
    missing: Vec<MissingSynthDef>,
}

/// Cap on unanswered `/s_new` requests kept for attribution. A request
/// that is still unanswered after this many later ones is not going to be
/// attributable; forgetting it bounds the queue on a long-running server.
const MAX_OUTSTANDING_SYNTHS: usize = 4096;

impl GraphIntegrity {
    fn sent(&mut self, node: NodeId, def: &str) {
        if self.outstanding.len() >= MAX_OUTSTANDING_SYNTHS {
            self.outstanding.pop_front();
        }
        self.outstanding.push_back((node, def.to_string()));
    }

    fn went_live(&mut self, node: NodeId) {
        if let Some(pos) = self.outstanding.iter().position(|(id, _)| *id == node) {
            self.outstanding.remove(pos);
        }
    }

    fn refused(&mut self, reason: &str) -> Option<MissingSynthDef> {
        let (node, def) = self.outstanding.pop_front()?;
        let missing = MissingSynthDef {
            def,
            node,
            reason: reason.to_string(),
        };
        self.missing.push(missing.clone());
        Some(missing)
    }
}

/// SuperCollider scsynth backend.
///
/// Communicates with scsynth via OSC over UDP.
pub struct ScsynthBackend {
    /// UDP socket for sending OSC messages.
    socket: Arc<UdpSocket>,
    /// Target server address.
    addr: String,
    /// Whether the listener thread should keep running.
    running: Arc<AtomicBool>,
    /// Channel sender for node end events (freed nodes, doneActions).
    #[allow(dead_code)] // Reserved for node lifecycle tracking
    node_end_tx: mpsc::Sender<NodeId>,
    /// Pending buffer info requests (buffer_id -> oneshot sender).
    pending_buffer_info: Arc<Mutex<HashMap<u32, oneshot::Sender<BufferInfo>>>>,
    /// Pending sync requests (sync_id -> oneshot sender).
    pending_sync: Arc<Mutex<HashMap<i32, oneshot::Sender<()>>>>,
    /// Pending control bus read requests (bus_index -> oneshot sender).
    pending_control_bus: Arc<Mutex<HashMap<u32, oneshot::Sender<f32>>>>,
    /// Pending command completion requests (command path -> oneshot sender).
    pending_done: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    /// Pending /d_recv synthdef load request.
    pending_synthdef_load: Arc<Mutex<Option<PendingSynthDefLoad>>>,
    /// Pending `/g_queryTree` request.
    pending_node_tree: Arc<Mutex<Option<oneshot::Sender<Vec<TreeSynth>>>>>,
    /// Next sync ID for /sync messages.
    next_sync_id: Arc<std::sync::atomic::AtomicI32>,
    /// General response callbacks.
    callbacks: Arc<Mutex<Vec<OscCallback>>>,
    /// Node IDs for which scsynth has delivered a definitive `/n_end`.
    /// Cleared before an ID is submitted for a new node generation.
    ended_nodes: Arc<Mutex<HashSet<NodeId>>>,
    /// `/s_new` requests the server has not answered, and the ones it refused.
    graph_integrity: Arc<Mutex<GraphIntegrity>>,
    /// Server status (updated by listener).
    server_ready: Arc<AtomicBool>,
}

impl std::fmt::Debug for ScsynthBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScsynthBackend")
            .field("addr", &self.addr)
            .field("server_ready", &self.server_ready.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl ScsynthBackend {
    /// Connect to scsynth at the given address.
    ///
    /// # Arguments
    ///
    /// * `addr` - Server address in "host:port" format (e.g., "127.0.0.1:57110")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let backend = ScsynthBackend::connect("127.0.0.1:57110").await?;
    /// ```
    pub async fn connect(addr: &str) -> Result<Self, ScsynthError> {
        // Bind to an ephemeral port
        let socket = UdpSocket::bind("0.0.0.0:0")?;

        // Set read timeout for non-blocking listener checks
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;

        // Connect to the server
        socket.connect(addr)?;

        let running = Arc::new(AtomicBool::new(true));
        let server_ready = Arc::new(AtomicBool::new(false));
        let (node_end_tx, _node_end_rx) = mpsc::channel(1024);
        let pending_buffer_info = Arc::new(Mutex::new(HashMap::new()));
        let pending_sync = Arc::new(Mutex::new(HashMap::new()));
        let pending_control_bus = Arc::new(Mutex::new(HashMap::new()));
        let pending_done = Arc::new(Mutex::new(HashMap::new()));
        let pending_synthdef_load = Arc::new(Mutex::new(None));
        let pending_node_tree = Arc::new(Mutex::new(None));
        let next_sync_id = Arc::new(std::sync::atomic::AtomicI32::new(1));
        let callbacks = Arc::new(Mutex::new(Vec::new()));
        let ended_nodes = Arc::new(Mutex::new(HashSet::new()));
        let graph_integrity = Arc::new(Mutex::new(GraphIntegrity::default()));

        let socket = Arc::new(socket);

        let backend = Self {
            socket: socket.clone(),
            addr: addr.to_string(),
            running: running.clone(),
            node_end_tx,
            pending_buffer_info: pending_buffer_info.clone(),
            pending_sync: pending_sync.clone(),
            pending_control_bus: pending_control_bus.clone(),
            pending_done: pending_done.clone(),
            pending_synthdef_load: pending_synthdef_load.clone(),
            pending_node_tree: pending_node_tree.clone(),
            next_sync_id,
            callbacks: callbacks.clone(),
            ended_nodes: ended_nodes.clone(),
            graph_integrity: graph_integrity.clone(),
            server_ready: server_ready.clone(),
        };

        // Record lifecycle notifications independently of the best-effort
        // State callback installed by the CLI. Voice teardown consults this
        // set before sending a fallback /n_free, so a contended State lock
        // cannot turn an already-observed /n_end into a server-side failure.
        backend.on_response(Arc::new(move |response| match response {
            OscResponse::NodeEnd { node_id, .. } => {
                if let Ok(mut ended) = ended_nodes.lock() {
                    ended.insert(node_id);
                }
            }
            OscResponse::NodeGo { node_id, .. } => {
                if let Ok(mut ended) = ended_nodes.lock() {
                    ended.remove(&node_id);
                }
                if let Ok(mut integrity) = graph_integrity.lock() {
                    integrity.went_live(node_id);
                }
            }
            // A refused `/s_new` is the only place a SynthDef the server
            // never built becomes observable: `/d_recv` answered `/done`
            // for it. Name the def, and drop its content hash so the next
            // eval re-sends the def instead of skipping it as unchanged —
            // otherwise one refusal silences that voice for the life of
            // the process, and no amount of reloading brings it back.
            OscResponse::Fail { command, reason } if command == "/s_new" => {
                let missing = graph_integrity
                    .lock()
                    .ok()
                    .and_then(|mut integrity| integrity.refused(&reason));
                match missing {
                    Some(missing) => {
                        tracing::error!(
                            "SynthDef '{}' is not on the server: node {} was not created ({}). \
                             The graph is incomplete — that voice is silent.",
                            missing.def,
                            missing.node.0,
                            missing.reason
                        );
                        vibelang_dsp::forget_synthdef_hash(&missing.def);
                    }
                    None => tracing::error!(
                        "scsynth refused an /s_new ({}) that no outstanding request \
                         could be attributed to — the graph is incomplete.",
                        reason
                    ),
                }
            }
            _ => {}
        }));

        // Start OSC listener thread
        Self::start_listener(
            socket.clone(),
            running.clone(),
            pending_buffer_info.clone(),
            pending_sync.clone(),
            pending_control_bus.clone(),
            pending_done.clone(),
            pending_synthdef_load.clone(),
            pending_node_tree.clone(),
            callbacks.clone(),
            server_ready.clone(),
        );

        // Enable notifications
        backend.send_msg("/notify", vec![OscType::Int(1)])?;

        // Wait for server to be ready
        backend.wait_for_server(Duration::from_secs(5)).await?;

        // scsynth is a separate process and survives `systemctl restart
        // vibelang.service`. Without this reset, link/voice synths from
        // the previous vibelang PID keep running and sum onto hardware
        // buses — the user-visible symptom is asymmetric audio (e.g. R-only
        // stereo) until a full host reboot. /g_freeAll on root group 0
        // frees every synth; /clearSched drops any pending bundle
        // schedule; /sync waits for both to complete before any later
        // commands (group/voice spawns) run against a now-empty tree.
        backend.send_msg("/g_freeAll", vec![OscType::Int(0)])?;
        backend.send_msg("/clearSched", vec![])?;
        backend.sync().await?;

        // The synthdef-body hash registry lets `deploy_synthdef_ir` skip
        // re-sending byte-identical defs across reloads. That cache mirrors
        // what *this* client believes the server already holds, so on a
        // (re)connect to a fresh server it must be dropped — otherwise a
        // within-process reconnect to a rebooted scsynth would hash-skip
        // every deploy and leave the new server with no synthdefs. Empty on
        // first connect (no-op); forces a full re-send on reconnect.
        vibelang_dsp::clear_synthdef_hash_registry();

        tracing::info!("Connected to scsynth at {}", addr);
        Ok(backend)
    }

    /// Create a new backend connected to the default address (localhost:57110).
    pub async fn default() -> Result<Self, ScsynthError> {
        Self::connect("127.0.0.1:57110").await
    }

    /// Wait for the server to be ready by polling /status.
    async fn wait_for_server(&self, timeout: Duration) -> Result<(), ScsynthError> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(100);

        while start.elapsed() < timeout {
            // Send status request
            if let Err(e) = self.send_msg("/status", vec![]) {
                tracing::debug!("Status request failed: {}", e);
            }

            // Wait a bit for response
            tokio::time::sleep(poll_interval).await;

            // Check if we got a status response
            if self.server_ready.load(Ordering::Relaxed) {
                return Ok(());
            }
        }

        Err(ScsynthError::ServerNotReady)
    }

    /// Start the OSC listener thread.
    fn start_listener(
        socket: Arc<UdpSocket>,
        running: Arc<AtomicBool>,
        pending_buffer_info: Arc<Mutex<HashMap<u32, oneshot::Sender<BufferInfo>>>>,
        pending_sync: Arc<Mutex<HashMap<i32, oneshot::Sender<()>>>>,
        pending_control_bus: Arc<Mutex<HashMap<u32, oneshot::Sender<f32>>>>,
        pending_done: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
        pending_synthdef_load: Arc<Mutex<Option<PendingSynthDefLoad>>>,
        pending_node_tree: Arc<Mutex<Option<oneshot::Sender<Vec<TreeSynth>>>>>,
        callbacks: Arc<Mutex<Vec<OscCallback>>>,
        server_ready: Arc<AtomicBool>,
    ) {
        // Clone socket for receiving (same socket, just Arc clone)
        let recv_socket = socket;

        thread::spawn(move || {
            let mut buf = [0u8; 8192];

            while running.load(Ordering::Relaxed) {
                // Try to receive a packet
                match recv_socket.recv(&mut buf) {
                    Ok(size) => {
                        // Parse OSC packet
                        if let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) {
                            Self::handle_packet(
                                packet,
                                &pending_buffer_info,
                                &pending_sync,
                                &pending_control_bus,
                                &pending_done,
                                &pending_synthdef_load,
                                &pending_node_tree,
                                &callbacks,
                                &server_ready,
                            );
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Timeout, continue polling
                        continue;
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                        // Timeout, continue polling
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("OSC receive error: {}", e);
                    }
                }
            }

            tracing::debug!("OSC listener thread stopped");
        });
    }

    /// Handle an incoming OSC packet.
    fn handle_packet(
        packet: OscPacket,
        pending_buffer_info: &Arc<Mutex<HashMap<u32, oneshot::Sender<BufferInfo>>>>,
        pending_sync: &Arc<Mutex<HashMap<i32, oneshot::Sender<()>>>>,
        pending_control_bus: &Arc<Mutex<HashMap<u32, oneshot::Sender<f32>>>>,
        pending_done: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
        pending_synthdef_load: &Arc<Mutex<Option<PendingSynthDefLoad>>>,
        pending_node_tree: &Arc<Mutex<Option<oneshot::Sender<Vec<TreeSynth>>>>>,
        callbacks: &Arc<Mutex<Vec<OscCallback>>>,
        server_ready: &Arc<AtomicBool>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(
                    msg,
                    pending_buffer_info,
                    pending_sync,
                    pending_control_bus,
                    pending_done,
                    pending_synthdef_load,
                    pending_node_tree,
                    callbacks,
                    server_ready,
                );
            }
            OscPacket::Bundle(bundle) => {
                for content in bundle.content {
                    Self::handle_packet(
                        content,
                        pending_buffer_info,
                        pending_sync,
                        pending_control_bus,
                        pending_done,
                        pending_synthdef_load,
                        pending_node_tree,
                        callbacks,
                        server_ready,
                    );
                }
            }
        }
    }

    /// Handle an incoming OSC message.
    fn handle_message(
        msg: OscMessage,
        pending_buffer_info: &Arc<Mutex<HashMap<u32, oneshot::Sender<BufferInfo>>>>,
        pending_sync: &Arc<Mutex<HashMap<i32, oneshot::Sender<()>>>>,
        pending_control_bus: &Arc<Mutex<HashMap<u32, oneshot::Sender<f32>>>>,
        pending_done: &Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
        pending_synthdef_load: &Arc<Mutex<Option<PendingSynthDefLoad>>>,
        pending_node_tree: &Arc<Mutex<Option<oneshot::Sender<Vec<TreeSynth>>>>>,
        callbacks: &Arc<Mutex<Vec<OscCallback>>>,
        server_ready: &Arc<AtomicBool>,
    ) {
        Self::osc_diag_log(|| {
            format!(
                "osc_recv path={} args=[{}]",
                msg.addr,
                Self::summarize_osc_args(&msg.args)
            )
        });
        let response = match msg.addr.as_str() {
            "/status.reply" => {
                // Parse status response
                server_ready.store(true, Ordering::Relaxed);

                if msg.args.len() >= 9 {
                    Some(OscResponse::Status {
                        num_ugens: Self::get_int(&msg.args, 1).unwrap_or(0),
                        num_synths: Self::get_int(&msg.args, 2).unwrap_or(0),
                        num_groups: Self::get_int(&msg.args, 3).unwrap_or(0),
                        num_synthdefs: Self::get_int(&msg.args, 4).unwrap_or(0),
                        avg_cpu: Self::get_float(&msg.args, 5).unwrap_or(0.0),
                        peak_cpu: Self::get_float(&msg.args, 6).unwrap_or(0.0),
                        sample_rate: Self::get_double(&msg.args, 7).unwrap_or(44100.0),
                        actual_sample_rate: Self::get_double(&msg.args, 8).unwrap_or(44100.0),
                    })
                } else {
                    None
                }
            }
            "/n_end" => {
                // Node ended
                if msg.args.len() >= 5 {
                    let node_id = NodeId::new(Self::get_int(&msg.args, 0).unwrap_or(0) as u32);
                    let parent_group = NodeId::new(Self::get_int(&msg.args, 1).unwrap_or(0) as u32);
                    let prev_node = NodeId::new(Self::get_int(&msg.args, 2).unwrap_or(-1) as u32);
                    let next_node = NodeId::new(Self::get_int(&msg.args, 3).unwrap_or(-1) as u32);
                    let is_group = Self::get_int(&msg.args, 4).unwrap_or(0) == 1;

                    tracing::trace!("Node {} ended (parent={})", node_id.0, parent_group.0);

                    Some(OscResponse::NodeEnd {
                        node_id,
                        parent_group,
                        prev_node,
                        next_node,
                        is_group,
                        head_node: None,
                        tail_node: None,
                    })
                } else {
                    None
                }
            }
            "/n_go" => {
                // Node started
                if msg.args.len() >= 5 {
                    let node_id = NodeId::new(Self::get_int(&msg.args, 0).unwrap_or(0) as u32);
                    let parent_group = NodeId::new(Self::get_int(&msg.args, 1).unwrap_or(0) as u32);
                    let prev_node = NodeId::new(Self::get_int(&msg.args, 2).unwrap_or(-1) as u32);
                    let next_node = NodeId::new(Self::get_int(&msg.args, 3).unwrap_or(-1) as u32);
                    let is_group = Self::get_int(&msg.args, 4).unwrap_or(0) == 1;

                    tracing::trace!("Node {} started (parent={})", node_id.0, parent_group.0);

                    Some(OscResponse::NodeGo {
                        node_id,
                        parent_group,
                        prev_node,
                        next_node,
                        is_group,
                        head_node: None,
                        tail_node: None,
                    })
                } else {
                    None
                }
            }
            "/b_info" => {
                // Buffer info response
                if msg.args.len() >= 4 {
                    let buffer_id = Self::get_int(&msg.args, 0).unwrap_or(0) as u32;
                    let frames = Self::get_int(&msg.args, 1).unwrap_or(0) as u32;
                    let channels = Self::get_int(&msg.args, 2).unwrap_or(2) as u16;
                    let sample_rate = Self::get_float(&msg.args, 3).unwrap_or(44100.0) as f64;

                    // Fulfill pending request if any
                    if let Ok(mut pending) = pending_buffer_info.lock() {
                        if let Some(sender) = pending.remove(&buffer_id) {
                            let _ = sender.send(BufferInfo {
                                frames,
                                channels,
                                sample_rate,
                            });
                        }
                    }

                    Some(OscResponse::BufferInfo {
                        buffer_id: BufferId::new(buffer_id),
                        frames,
                        channels,
                        sample_rate,
                    })
                } else {
                    None
                }
            }
            "/g_queryTree.reply" => {
                if let Ok(mut pending) = pending_node_tree.lock() {
                    if let Some(sender) = pending.take() {
                        match Self::parse_node_tree(&msg.args) {
                            Some(tree) => {
                                let _ = sender.send(tree);
                            }
                            None => {
                                tracing::debug!(
                                    "Unparseable /g_queryTree.reply ({} args) — dropping",
                                    msg.args.len()
                                );
                            }
                        }
                    }
                }
                None
            }
            "/done" => {
                // Command completed
                let command = Self::get_string(&msg.args, 0).unwrap_or_default();
                tracing::debug!("Done: {}", command);
                if Self::is_pending_synthdef_command(&command) {
                    if let Ok(mut pending) = pending_synthdef_load.lock() {
                        if pending
                            .as_ref()
                            .map(|pending| pending.command == command)
                            .unwrap_or(false)
                        {
                            let pending = pending.take().expect("pending synthdef load checked");
                            tracing::debug!("SynthDef '{}' accepted by scsynth", pending.name);
                            let _ = pending.tx.send(SynthDefLoadReply::Done);
                        }
                    }
                }
                if let Ok(mut pending) = pending_done.lock() {
                    // Async buffer commands (/b_allocRead, /b_alloc, ...)
                    // echo the buffer number as a second /done argument.
                    // Try the bufnum-correlated key first so concurrent
                    // loads of different buffers each complete their own
                    // waiter; fall back to the plain command key.
                    let keyed_sender = Self::get_int(&msg.args, 1).and_then(|bufnum| {
                        pending.remove(&Self::done_key_for_buffer(&command, bufnum as u32))
                    });
                    if let Some(sender) = keyed_sender {
                        let _ = sender.send(());
                    } else if let Some(sender) = pending.remove(&command) {
                        let _ = sender.send(());
                    }
                }
                Some(OscResponse::Done { command })
            }
            "/fail" => {
                // Command failed
                let command = Self::get_string(&msg.args, 0).unwrap_or_default();
                let reason = Self::get_string(&msg.args, 1).unwrap_or_default();

                // Node not found on n_free/n_set is expected for synths with doneAction=2
                // (they free themselves when their envelope completes)
                let is_expected_node_not_found =
                    (command == "/n_free" || command == "/n_set") && reason.contains("not found");

                if is_expected_node_not_found {
                    tracing::trace!(
                        "Expected: {} - {} (synth already freed via doneAction)",
                        command,
                        reason
                    );
                } else if command == "/s_new" {
                    // Reported by the graph-integrity callback instead, which
                    // can name the SynthDef and node this bare reason cannot.
                    tracing::debug!("Fail: {} - {}", command, reason);
                } else {
                    tracing::warn!("Fail: {} - {}", command, reason);
                }
                if Self::is_pending_synthdef_command(&command) {
                    if let Ok(mut pending) = pending_synthdef_load.lock() {
                        if pending
                            .as_ref()
                            .map(|pending| pending.command == command)
                            .unwrap_or(false)
                        {
                            let pending = pending.take().expect("pending synthdef load checked");
                            tracing::warn!(
                                "SynthDef '{}' rejected by scsynth: {}",
                                pending.name,
                                reason
                            );
                            let _ = pending.tx.send(SynthDefLoadReply::Fail {
                                reason: reason.clone(),
                            });
                        }
                    }
                }
                Some(OscResponse::Fail { command, reason })
            }
            "/synced" => {
                // Sync completed
                if let Some(sync_id) = Self::get_int(&msg.args, 0) {
                    tracing::debug!("Synced: {}", sync_id);
                    // Fulfill pending sync request
                    if let Ok(mut pending) = pending_sync.lock() {
                        if let Some(sender) = pending.remove(&sync_id) {
                            let _ = sender.send(());
                        }
                    }
                }
                None // Don't propagate to callbacks
            }
            "/c_set" => {
                // Control bus value response (from /c_get)
                // Format: [bus_index, value] pairs (can have multiple)
                // We handle one at a time since we send /c_get for single buses
                if msg.args.len() >= 2 {
                    let bus_index = Self::get_int(&msg.args, 0).unwrap_or(0) as u32;
                    let value = Self::get_float(&msg.args, 1).unwrap_or(0.0);

                    tracing::trace!("Control bus {}: {}", bus_index, value);

                    // Fulfill pending request if any
                    if let Ok(mut pending) = pending_control_bus.lock() {
                        if let Some(sender) = pending.remove(&bus_index) {
                            let _ = sender.send(value);
                        }
                    }
                }
                None // Don't propagate to callbacks
            }
            "/tr" => {
                // Trigger message (SendTrig from synth)
                // Format: node_id (int), trig_id (int), value (float)
                if msg.args.len() >= 3 {
                    let node_id = NodeId::new(Self::get_int(&msg.args, 0).unwrap_or(0) as u32);
                    let trig_id = Self::get_int(&msg.args, 1).unwrap_or(0);
                    let value = Self::get_float(&msg.args, 2).unwrap_or(0.0);

                    // Don't log meter triggers (too noisy at 20Hz)
                    if trig_id >= 100 {
                        tracing::trace!(
                            "Trigger: node={} id={} value={}",
                            node_id.0,
                            trig_id,
                            value
                        );
                    }

                    Some(OscResponse::Trigger {
                        node_id,
                        trig_id,
                        value,
                    })
                } else {
                    None
                }
            }
            _ => {
                tracing::trace!("Unknown OSC message: {} {:?}", msg.addr, msg.args);
                Some(OscResponse::Unknown {
                    path: msg.addr,
                    args: msg.args,
                })
            }
        };

        // Notify callbacks
        if let Some(response) = response {
            if let Ok(cbs) = callbacks.lock() {
                for cb in cbs.iter() {
                    cb(response.clone());
                }
            }
        }
    }

    // Helper methods for extracting OSC values

    fn get_int(args: &[OscType], idx: usize) -> Option<i32> {
        args.get(idx).and_then(|v| match v {
            OscType::Int(i) => Some(*i),
            _ => None,
        })
    }

    fn get_float(args: &[OscType], idx: usize) -> Option<f32> {
        args.get(idx).and_then(|v| match v {
            OscType::Float(f) => Some(*f),
            _ => None,
        })
    }

    fn get_double(args: &[OscType], idx: usize) -> Option<f64> {
        args.get(idx).and_then(|v| match v {
            OscType::Double(d) => Some(*d),
            OscType::Float(f) => Some(*f as f64),
            _ => None,
        })
    }

    fn get_string(args: &[OscType], idx: usize) -> Option<String> {
        args.get(idx).and_then(|v| match v {
            OscType::String(s) => Some(s.clone()),
            _ => None,
        })
    }

    /// Note an `/s_new` about to go out, so a later `/fail` can name it.
    fn record_synth_request(&self, node: NodeId, def: &str) {
        if let Ok(mut integrity) = self.graph_integrity.lock() {
            integrity.sent(node, def);
        }
    }

    /// Ask the server for its whole node tree and wait for the answer.
    ///
    /// The reply lists nodes depth-first, which IS the order scsynth
    /// evaluates them in, so the returned vector is in evaluation order.
    /// Only synths are returned — groups carry no controls and read no bus.
    pub async fn query_node_tree(&self) -> Result<Vec<TreeSynth>, ScsynthError> {
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self
                .pending_node_tree
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            *pending = Some(tx);
        }

        // Group 0 (the root), flag 1 = include control values. The bus a node
        // reads and writes lives in those controls, so flag 0 is useless here.
        self.send_msg("/g_queryTree", vec![OscType::Int(0), OscType::Int(1)])?;

        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(tree)) => Ok(tree),
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
                "Response channel closed".to_string(),
            )),
            Err(_) => Err(ScsynthError::Timeout),
        }
    }

    /// The live nodes whose audio output is dropped on the floor.
    ///
    /// Asks the server for its node tree and walks it in evaluation order.
    /// scsynth zeroes audio buses every block, so a node that writes a bus
    /// only read EARLIER in the same cycle is writing into nothing: whatever
    /// feeds it is inaudible, however healthy every synth involved looks.
    /// See [`audio_path_breaks`] for the rule and its limits.
    ///
    /// Errors are the caller's cue to stay quiet, not to claim a break: an
    /// unreadable tree is not evidence of a broken one.
    pub async fn audio_path_breaks(&self) -> Result<Vec<AudioPathBreak>, ScsynthError> {
        Ok(audio_path_breaks(&self.query_node_tree().await?))
    }

    /// Take the SynthDefs the server has refused to instantiate since the
    /// last call, and reset the ledger.
    ///
    /// A caller that has just applied a reload uses this to decide whether
    /// the graph it built is whole. Call it AFTER a `/sync` barrier: the
    /// `/fail` for an `/s_new` reaches this process before the `/synced`
    /// that follows it, so a synced reload has already been judged.
    pub fn take_missing_synthdefs(&self) -> Vec<MissingSynthDef> {
        match self.graph_integrity.lock() {
            Ok(mut integrity) => std::mem::take(&mut integrity.missing),
            Err(_) => Vec::new(),
        }
    }

    /// Parse a `/g_queryTree.reply` argument list into its synths, in the
    /// depth-first order the server reports (and evaluates) them.
    ///
    /// Wire format: `flag, root node, root child count`, then per node
    /// `node id, child count` — child count is -1 for a synth, and a synth
    /// adds its def name plus, when `flag` is 1, `numControls` and that many
    /// name/value pairs. Groups contribute neither, and their children simply
    /// follow inline, so a flat scan needs no tree bookkeeping.
    ///
    /// Returns `None` on a reply without control values or on any malformed
    /// tail: half a tree cannot be reasoned about, and guessing at one would
    /// invent breaks.
    fn parse_node_tree(args: &[OscType]) -> Option<Vec<TreeSynth>> {
        let int_at = |i: usize| match args.get(i) {
            Some(OscType::Int(v)) => Some(*v),
            _ => None,
        };
        if int_at(0)? != 1 {
            return None;
        }

        let mut synths = Vec::new();
        let mut i = 3;
        while i < args.len() {
            let node = int_at(i)?;
            let children = int_at(i + 1)?;
            i += 2;
            if children >= 0 {
                continue; // a group: no def name, no controls
            }

            let def = match args.get(i)? {
                OscType::String(name) => name.clone(),
                _ => return None,
            };
            i += 1;

            let num_controls = int_at(i)?;
            i += 1;
            let mut controls = HashMap::new();
            for _ in 0..num_controls {
                let name = match args.get(i)? {
                    OscType::String(name) => name.clone(),
                    OscType::Int(index) => index.to_string(),
                    _ => return None,
                };
                let value = match args.get(i + 1)? {
                    OscType::Float(v) => Some(*v),
                    OscType::Double(v) => Some(*v as f32),
                    // A control mapped to a bus reports as e.g. "c12".
                    OscType::String(_) => None,
                    _ => return None,
                };
                i += 2;
                if let Some(value) = value {
                    controls.insert(name, value);
                }
            }

            synths.push(TreeSynth {
                node,
                def,
                controls,
            });
        }
        Some(synths)
    }

    fn clear_pending_synthdef_load(&self, name: &str) {
        if let Ok(mut pending) = self.pending_synthdef_load.lock() {
            if pending
                .as_ref()
                .map(|pending| pending.name == name)
                .unwrap_or(false)
            {
                *pending = None;
            }
        }
    }

    fn parse_missing_ugen(reason: &str) -> Option<String> {
        let (_, rest) = reason.split_once("UGen '")?;
        let (ugen, rest) = rest.split_once('\'')?;
        rest.contains("not installed").then(|| ugen.to_string())
    }

    fn is_pending_synthdef_command(command: &str) -> bool {
        command == "/d_recv" || command == "/d_load"
    }

    /// Whether OSC diagnostics are enabled (`VIBELANG_LOG_OSC=1`).
    ///
    /// Cached in a `OnceLock`: this sits on the per-message send/receive
    /// hot path, so it must cost a single atomic load — not an env lookup.
    fn osc_diag_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("VIBELANG_LOG_OSC")
                .map(|value| {
                    let value = value.trim();
                    value == "1"
                        || value.eq_ignore_ascii_case("true")
                        || value.eq_ignore_ascii_case("yes")
                        || value.eq_ignore_ascii_case("on")
                })
                .unwrap_or(false)
        })
    }

    fn osc_diag_timestamp_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    }

    /// Emit an OSC diagnostic line, formatting lazily.
    ///
    /// Takes a closure so callers pay zero formatting/summarization cost
    /// when diagnostics are disabled (the common case — this runs for
    /// every OSC message sent and received).
    fn osc_diag_log<F, S>(message: F)
    where
        F: FnOnce() -> S,
        S: AsRef<str>,
    {
        if Self::osc_diag_enabled() {
            eprintln!(
                "[vibelang-osc-diag t_ms={}] {}",
                Self::osc_diag_timestamp_ms(),
                message().as_ref()
            );
        }
    }

    fn summarize_osc_arg(arg: &OscType) -> String {
        match arg {
            OscType::Int(value) => value.to_string(),
            OscType::Float(value) => format!("{value:.6}"),
            OscType::String(value) => {
                if value.len() > 80 {
                    format!("{:?}...", &value[..80])
                } else {
                    format!("{value:?}")
                }
            }
            OscType::Blob(value) => format!("blob({} bytes)", value.len()),
            OscType::Time(_) => "time".to_string(),
            OscType::Long(value) => value.to_string(),
            OscType::Double(value) => format!("{value:.6}"),
            OscType::Char(value) => value.to_string(),
            OscType::Color(_) => "color".to_string(),
            OscType::Midi(_) => "midi".to_string(),
            OscType::Bool(value) => value.to_string(),
            OscType::Array(value) => format!("array({} items)", value.content.len()),
            OscType::Nil => "nil".to_string(),
            OscType::Inf => "inf".to_string(),
        }
    }

    fn summarize_osc_args(args: &[OscType]) -> String {
        const MAX_ARGS: usize = 12;
        let mut parts: Vec<String> = args
            .iter()
            .take(MAX_ARGS)
            .map(Self::summarize_osc_arg)
            .collect();
        if args.len() > MAX_ARGS {
            parts.push(format!("... {} more", args.len() - MAX_ARGS));
        }
        parts.join(", ")
    }

    fn temp_synthdef_path(name: &str) -> PathBuf {
        let mut safe_name = String::with_capacity(name.len());
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                safe_name.push(ch);
            } else {
                safe_name.push('_');
            }
        }
        if safe_name.is_empty() {
            safe_name.push_str("synthdef");
        }

        let counter = TEMP_SYNTHDEF_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "vibelang-{}-{}-{}.scsyndef",
            std::process::id(),
            counter,
            safe_name
        ))
    }

    async fn await_synthdef_load(
        &self,
        name: &str,
        rx: oneshot::Receiver<SynthDefLoadReply>,
    ) -> Result<(), ScsynthError> {
        match tokio::time::timeout(SYNTHDEF_LOAD_TIMEOUT, rx).await {
            Ok(Ok(SynthDefLoadReply::Done)) => Ok(()),
            Ok(Ok(SynthDefLoadReply::Fail { reason })) => Err(ScsynthError::SynthDefRejected {
                name: name.to_string(),
                missing_ugen: Self::parse_missing_ugen(&reason),
                reason,
            }),
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
                "synthdef load response channel closed".to_string(),
            )),
            Err(_) => {
                self.clear_pending_synthdef_load(name);
                tracing::warn!(
                    "SynthDef '{}' load timed out after {:?}",
                    name,
                    SYNTHDEF_LOAD_TIMEOUT
                );
                Err(ScsynthError::Timeout)
            }
        }
    }

    /// Send an OSC message to scsynth.
    fn send_msg(&self, path: &str, args: Vec<OscType>) -> Result<(), ScsynthError> {
        Self::osc_diag_log(|| {
            format!(
                "osc_send path={} args=[{}]",
                path,
                Self::summarize_osc_args(&args)
            )
        });
        let msg = OscMessage {
            addr: path.to_string(),
            args,
        };
        let packet = OscPacket::Message(msg);
        let buf = encoder::encode(&packet)?;
        self.socket.send(&buf)?;
        Ok(())
    }

    /// Convert a monotonic deadline into an OSC/NTP time tag.
    ///
    /// The runtime's lookahead schedulers produce deadlines on the monotonic
    /// clock (`Instant`). scsynth wants absolute NTP wall time in bundle
    /// time-tags, so we anchor the remaining lead time (`at - now_mono`)
    /// onto the wall clock (`now_wall + lead`). Doing the anchoring at send
    /// time keeps the conversion immune to wall-clock jumps between
    /// scheduling and dispatch.
    ///
    /// Returns `None` when `at` is not strictly in the future — the caller
    /// must then send immediately (the OSC "execute now" idiom) instead of
    /// emitting a late bundle that scsynth would warn about.
    fn schedule_time_tag(at: Instant, now_mono: Instant, now_wall: SystemTime) -> Option<OscTime> {
        let lead = at.checked_duration_since(now_mono)?;
        if lead.is_zero() {
            return None;
        }
        OscTime::try_from(now_wall + lead).ok()
    }

    /// Encode a batch of messages as a single time-tagged OSC bundle.
    ///
    /// scsynth executes the bundle's messages in order, sample-accurately at
    /// the tagged time — which is also why `/s_new` and its `/n_map`s must
    /// travel in one bundle (the node only exists once the bundle runs).
    fn encode_bundle(timetag: OscTime, msgs: Vec<OscMessage>) -> Result<Vec<u8>, ScsynthError> {
        let packet = OscPacket::Bundle(OscBundle {
            timetag,
            content: msgs.into_iter().map(OscPacket::Message).collect(),
        });
        Ok(encoder::encode(&packet)?)
    }

    /// Send a batch of OSC messages, optionally scheduled at `at`.
    ///
    /// With a future deadline the messages are wrapped in one OSC bundle
    /// carrying an NTP time-tag, so scsynth executes them sample-accurately
    /// at that time. With no deadline — or one that has already passed by
    /// send time — the messages go out immediately as plain messages
    /// (equivalent to time-tag 1, "now"), avoiding scsynth "late" warnings.
    fn send_msgs_at(
        &self,
        msgs: Vec<(&'static str, Vec<OscType>)>,
        at: Option<Instant>,
    ) -> Result<(), ScsynthError> {
        let timetag =
            at.and_then(|at| Self::schedule_time_tag(at, Instant::now(), SystemTime::now()));

        match timetag {
            Some(timetag) => {
                let msgs: Vec<OscMessage> = msgs
                    .into_iter()
                    .map(|(path, args)| {
                        Self::osc_diag_log(|| {
                            format!(
                                "osc_send_bundled tag=({},{}) path={} args=[{}]",
                                timetag.seconds,
                                timetag.fractional,
                                path,
                                Self::summarize_osc_args(&args)
                            )
                        });
                        OscMessage {
                            addr: path.to_string(),
                            args,
                        }
                    })
                    .collect();
                let buf = Self::encode_bundle(timetag, msgs)?;
                self.socket.send(&buf)?;
                Ok(())
            }
            None => {
                for (path, args) in msgs {
                    self.send_msg(path, args)?;
                }
                Ok(())
            }
        }
    }

    /// Build the `/s_new` argument list shared by the immediate and
    /// scheduled synth-creation paths.
    fn s_new_args(
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
    ) -> Vec<OscType> {
        let mut args: Vec<OscType> = vec![
            OscType::String(def.to_string()),
            OscType::Int(node.0 as i32),
            OscType::Int(action.to_sc_int()),
            OscType::Int(target.0 as i32),
        ];
        for (key, value) in params {
            args.push(OscType::String(key.clone()));
            args.push(OscType::Float(*value));
        }
        args
    }

    async fn send_msg_and_await_done(
        &self,
        path: &str,
        args: Vec<OscType>,
    ) -> Result<(), ScsynthError> {
        self.send_msg_and_await_done_key(path, path, args, Duration::from_secs(5))
            .await
    }

    /// The `pending_done` key for a buffer-async command's `/done` reply.
    ///
    /// scsynth echoes the buffer number as the second `/done` argument for
    /// asynchronous buffer commands, so waiters can be correlated
    /// per-buffer instead of per-command — required for concurrent
    /// `/b_allocRead`s (staged reload loads overlap deliberately).
    fn done_key_for_buffer(command: &str, bufnum: u32) -> String {
        format!("{} {}", command, bufnum)
    }

    /// Send `path` and wait for its `/done` reply registered under
    /// `done_key` (either the bare command, or a bufnum-correlated key from
    /// [`Self::done_key_for_buffer`]).
    async fn send_msg_and_await_done_key(
        &self,
        path: &str,
        done_key: &str,
        args: Vec<OscType>,
        timeout: Duration,
    ) -> Result<(), ScsynthError> {
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_done
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            if pending.insert(done_key.to_string(), tx).is_some() {
                return Err(ScsynthError::ConnectionFailed(format!(
                    "command already pending: {}",
                    done_key
                )));
            }
        }

        if let Err(err) = self.send_msg(path, args) {
            if let Ok(mut pending) = self.pending_done.lock() {
                pending.remove(done_key);
            }
            return Err(err);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(format!(
                "{} completion response channel closed",
                done_key
            ))),
            Err(_) => {
                if let Ok(mut pending) = self.pending_done.lock() {
                    pending.remove(done_key);
                }
                Err(ScsynthError::Timeout)
            }
        }
    }

    /// Get the server address.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Check if the server is ready.
    pub fn is_ready(&self) -> bool {
        self.server_ready.load(Ordering::Relaxed)
    }

    /// Register a callback for OSC responses.
    pub fn on_response(&self, callback: OscCallback) {
        if let Ok(mut cbs) = self.callbacks.lock() {
            cbs.push(callback);
        }
    }

    fn prepare_node_generation(&self, node: NodeId) {
        if let Ok(mut ended) = self.ended_nodes.lock() {
            ended.remove(&node);
        }
    }

    /// Request buffer info and wait for response.
    pub async fn query_buffer_info(&self, id: BufferId) -> Result<BufferInfo, ScsynthError> {
        // Create oneshot channel for response
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self
                .pending_buffer_info
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            pending.insert(id.0, tx);
        }

        // Send query
        self.send_msg("/b_query", vec![OscType::Int(id.0 as i32)])?;

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(info)) => Ok(info),
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
                "Response channel closed".to_string(),
            )),
            Err(_) => Err(ScsynthError::Timeout),
        }
    }

    /// Send /sync and wait for scsynth to process all pending commands.
    ///
    /// This ensures all previously sent commands have been processed before
    /// continuing. Useful for ensuring groups exist before creating synths
    /// that target them.
    pub async fn sync(&self) -> Result<(), ScsynthError> {
        // Get a unique sync ID
        let sync_id = self.next_sync_id.fetch_add(1, Ordering::SeqCst);
        let started = Instant::now();

        // Create oneshot channel for response
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self
                .pending_sync
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            pending.insert(sync_id, tx);
        }

        // Send sync command
        self.send_msg("/sync", vec![OscType::Int(sync_id)])?;
        Self::osc_diag_log(|| format!("sync_wait_start id={sync_id}"));

        // Wait for response with reasonable timeout.
        // Since MIDI clock is now handled in a dedicated thread, we don't need
        // aggressive timeouts here anymore.
        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(())) => {
                Self::osc_diag_log(|| {
                    format!(
                        "sync_wait_done id={} elapsed_ms={:.3}",
                        sync_id,
                        started.elapsed().as_secs_f64() * 1000.0
                    )
                });
                tracing::trace!("Sync {} completed", sync_id);
                Ok(())
            }
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
                "Sync response channel closed".to_string(),
            )),
            Err(_) => {
                Self::osc_diag_log(|| {
                    format!(
                        "sync_wait_timeout id={} elapsed_ms={:.3}",
                        sync_id,
                        started.elapsed().as_secs_f64() * 1000.0
                    )
                });
                tracing::warn!("Sync {} timed out after 5 seconds", sync_id);
                Err(ScsynthError::Timeout)
            }
        }
    }
}

impl Drop for ScsynthBackend {
    fn drop(&mut self) {
        // Signal listener thread to stop
        self.running.store(false, Ordering::Relaxed);
    }
}

#[async_trait]
impl Backend for ScsynthBackend {
    type Error = ScsynthError;

    async fn load_synthdef(&self, name: &str, data: &[u8]) -> Result<(), Self::Error> {
        let use_disk_load = data.len() > D_RECV_MAX_BYTES;
        let command = if use_disk_load { "/d_load" } else { "/d_recv" };
        ScsynthBackend::osc_diag_log(|| {
            format!(
                "synthdef_load name={} bytes={} command={} over_32k={} over_64k={}",
                name,
                data.len(),
                command,
                data.len() > 32 * 1024,
                data.len() >= 64 * 1024
            )
        });
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_synthdef_load
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            if pending.is_some() {
                return Err(ScsynthError::ConnectionFailed(
                    "concurrent /d_recv loads are not supported yet".to_string(),
                ));
            }
            *pending = Some(PendingSynthDefLoad {
                name: name.to_string(),
                command,
                tx,
            });
        }

        let temp_path = if use_disk_load {
            let path = Self::temp_synthdef_path(name);
            if let Err(err) = fs::write(&path, data) {
                self.clear_pending_synthdef_load(name);
                return Err(ScsynthError::Io(err));
            }
            tracing::debug!(
                "SynthDef '{}' is {} bytes; loading via /d_load from {}",
                name,
                data.len(),
                path.display()
            );
            Some(path)
        } else {
            None
        };

        let send_result = if let Some(path) = &temp_path {
            self.send_msg(
                "/d_load",
                vec![OscType::String(path.to_string_lossy().into_owned())],
            )
        } else {
            self.send_msg("/d_recv", vec![OscType::Blob(data.to_vec())])
        };

        if let Err(err) = send_result {
            self.clear_pending_synthdef_load(name);
            if let Some(path) = &temp_path {
                let _ = fs::remove_file(path);
            }
            return Err(err);
        }

        let result = self.await_synthdef_load(name, rx).await;
        if let Some(path) = &temp_path {
            if let Err(err) = fs::remove_file(path) {
                tracing::debug!(
                    "Failed to remove temporary SynthDef file {}: {}",
                    path.display(),
                    err
                );
            }
        }
        result
    }

    async fn create_synth(
        &self,
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.prepare_node_generation(node);
        tracing::debug!(
            "s_new: def='{}', node={}, target={}, params={:?}",
            def,
            node.0,
            target.0,
            params
        );

        self.record_synth_request(node, def);
        self.send_msg(
            "/s_new",
            Self::s_new_args(def, node, target, action, params),
        )?;
        Ok(())
    }

    async fn create_synth_at(
        &self,
        def: &str,
        node: NodeId,
        target: NodeId,
        action: AddAction,
        params: &ParamMap,
        param_buses: &[(String, u32)],
        at: Option<Instant>,
    ) -> Result<(), Self::Error> {
        self.prepare_node_generation(node);
        tracing::debug!(
            "s_new (scheduled): def='{}', node={}, target={}, at={:?}, params={:?}",
            def,
            node.0,
            target.0,
            at,
            params
        );

        self.record_synth_request(node, def);
        let mut msgs = vec![(
            "/s_new",
            Self::s_new_args(def, node, target, action, params),
        )];
        for (param, bus) in param_buses {
            msgs.push((
                "/n_map",
                vec![
                    OscType::Int(node.0 as i32),
                    OscType::String(param.clone()),
                    OscType::Int(*bus as i32),
                ],
            ));
        }

        self.send_msgs_at(msgs, at)
    }

    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error> {
        self.prepare_node_generation(node);
        self.send_msg(
            "/g_new",
            vec![
                OscType::Int(node.0 as i32),
                OscType::Int(action.to_sc_int()),
                OscType::Int(target.0 as i32),
            ],
        )?;
        Ok(())
    }

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        if self.node_has_ended(node) {
            tracing::debug!(
                "Skipping /n_free for node {} after definitive /n_end",
                node.0
            );
            return Ok(());
        }
        self.send_msg("/n_free", vec![OscType::Int(node.0 as i32)])?;
        Ok(())
    }

    fn node_has_ended(&self, node: NodeId) -> bool {
        self.ended_nodes
            .lock()
            .map(|ended| ended.contains(&node))
            .unwrap_or(false)
    }

    async fn run_node(&self, node: NodeId, running: bool) -> Result<(), Self::Error> {
        self.send_msg(
            "/n_run",
            vec![
                OscType::Int(node.0 as i32),
                OscType::Int(if running { 1 } else { 0 }),
            ],
        )?;
        Ok(())
    }

    async fn move_node_before(&self, node: NodeId, before: NodeId) -> Result<(), Self::Error> {
        self.send_msg(
            "/n_before",
            vec![OscType::Int(node.0 as i32), OscType::Int(before.0 as i32)],
        )?;
        Ok(())
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        self.send_msg(
            "/n_set",
            vec![
                OscType::Int(node.0 as i32),
                OscType::String(param.to_string()),
                OscType::Float(value),
            ],
        )?;
        Ok(())
    }

    async fn set_param_at(
        &self,
        node: NodeId,
        param: &str,
        value: f32,
        at: Option<Instant>,
    ) -> Result<(), Self::Error> {
        self.send_msgs_at(
            vec![(
                "/n_set",
                vec![
                    OscType::Int(node.0 as i32),
                    OscType::String(param.to_string()),
                    OscType::Float(value),
                ],
            )],
            at,
        )
    }

    async fn map_param_to_bus(
        &self,
        node: NodeId,
        param: &str,
        bus: u32,
    ) -> Result<(), Self::Error> {
        tracing::debug!("n_map: node={}, param='{}', bus={}", node.0, param, bus);
        self.send_msg(
            "/n_map",
            vec![
                OscType::Int(node.0 as i32),
                OscType::String(param.to_string()),
                OscType::Int(bus as i32),
            ],
        )?;
        Ok(())
    }

    async fn get_control_bus(&self, bus: u32) -> Result<f32, Self::Error> {
        // Create oneshot channel for the response
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self
                .pending_control_bus
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            pending.insert(bus, tx);
        }

        // Send /c_get command
        // scsynth responds with /c_set [bus_index, value]
        self.send_msg("/c_get", vec![OscType::Int(bus as i32)])?;

        // Wait for response with a short timeout
        // Control bus reads should be fast, use 50ms timeout
        match tokio::time::timeout(Duration::from_millis(50), rx).await {
            Ok(Ok(value)) => {
                tracing::trace!("Got control bus {} value: {}", bus, value);
                Ok(value)
            }
            Ok(Err(_)) => {
                // Channel closed, return default
                tracing::warn!("Control bus {} read cancelled", bus);
                Ok(0.0)
            }
            Err(_) => {
                // Timeout - remove pending request and return default
                {
                    let mut pending = self
                        .pending_control_bus
                        .lock()
                        .map_err(|_| ScsynthError::LockPoisoned)?;
                    pending.remove(&bus);
                }
                tracing::trace!("Control bus {} read timeout, returning 0.0", bus);
                Ok(0.0)
            }
        }
    }

    async fn get_control_buses(&self, buses: &[u32]) -> Result<HashMap<u32, f32>, Self::Error> {
        if buses.is_empty() {
            return Ok(HashMap::new());
        }

        // Create receivers for all buses
        let mut receivers: Vec<(u32, oneshot::Receiver<f32>)> = Vec::with_capacity(buses.len());

        // Register all pending requests and send all /c_get commands
        {
            let mut pending = self
                .pending_control_bus
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            for &bus in buses {
                let (tx, rx) = oneshot::channel();
                pending.insert(bus, tx);
                receivers.push((bus, rx));
            }
        }

        // Send all /c_get commands (non-blocking, just UDP sends)
        for &bus in buses {
            if let Err(e) = self.send_msg("/c_get", vec![OscType::Int(bus as i32)]) {
                tracing::warn!("Failed to send /c_get for bus {}: {}", bus, e);
            }
        }

        // Wait for all responses with a single timeout
        // Give 10ms base + 2ms per bus (accounts for scsynth processing time)
        let timeout_ms = 10 + (buses.len() as u64 * 2);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

        let mut results = HashMap::with_capacity(buses.len());

        for (bus, rx) in receivers {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                // Timeout exhausted, clean up remaining
                let mut pending = self
                    .pending_control_bus
                    .lock()
                    .map_err(|_| ScsynthError::LockPoisoned)?;
                pending.remove(&bus);
                continue;
            }

            match tokio::time::timeout(remaining, rx).await {
                Ok(Ok(value)) => {
                    results.insert(bus, value);
                }
                Ok(Err(_)) => {
                    // Channel closed
                }
                Err(_) => {
                    // Timeout - clean up
                    let mut pending = self
                        .pending_control_bus
                        .lock()
                        .map_err(|_| ScsynthError::LockPoisoned)?;
                    pending.remove(&bus);
                }
            }
        }

        tracing::trace!(
            "Batch read {} control buses, got {} responses",
            buses.len(),
            results.len()
        );

        Ok(results)
    }

    async fn load_buffer(&self, id: BufferId, path: &Path) -> Result<BufferInfo, Self::Error> {
        let path_str = path.to_str().ok_or_else(|| {
            ScsynthError::Io(io::Error::new(io::ErrorKind::InvalidInput, "Invalid path"))
        })?;

        // /b_allocRead is asynchronous (NRT thread), while /b_query answers
        // immediately on the RT thread — querying without awaiting the
        // /done would race the load and report stale metadata (0 frames on
        // a fresh buffer, or the PREVIOUS contents of a reused buffer ID).
        // The /done reply echoes the bufnum, so waiters are correlated
        // per-buffer and concurrent loads (staged reloads) don't collide.
        // Generous timeout: large sample files legitimately take a while.
        self.send_msg_and_await_done_key(
            "/b_allocRead",
            &Self::done_key_for_buffer("/b_allocRead", id.0),
            vec![
                OscType::Int(id.0 as i32),
                OscType::String(path_str.to_string()),
                OscType::Int(0),  // start frame
                OscType::Int(-1), // num frames (-1 = all)
            ],
            Duration::from_secs(30),
        )
        .await?;

        // scsynth sends /done /b_allocRead but not /b_info — query explicitly.
        let info = self.query_buffer_info(id).await?;
        tracing::debug!(
            "Loaded buffer {} from {:?} ({} frames, {} channels, {}Hz)",
            id.0,
            path,
            info.frames,
            info.channels,
            info.sample_rate
        );
        Ok(info)
    }

    async fn alloc_buffer(
        &self,
        id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, Self::Error> {
        self.send_msg_and_await_done(
            "/b_alloc",
            vec![
                OscType::Int(id.0 as i32),
                OscType::Int(frames as i32),
                OscType::Int(channels as i32),
            ],
        )
        .await?;

        // /b_alloc and /b_zero are asynchronous. Wait for their /done replies
        // before querying metadata; /sync alone can race the allocation.
        let query_result = match self
            .send_msg_and_await_done("/b_zero", vec![OscType::Int(id.0 as i32)])
            .await
        {
            Ok(()) => self.query_buffer_info(id).await,
            Err(err) => Err(err),
        };

        match query_result {
            Ok(info) => {
                tracing::debug!(
                    "Allocated buffer {} ({} frames, {} channels)",
                    id.0,
                    info.frames,
                    info.channels
                );
                Ok(info)
            }
            Err(err) => {
                tracing::warn!(
                    "Buffer {} alloc metadata query failed after allocation: {}, using allocated values",
                    id.0,
                    err
                );
                Ok(BufferInfo {
                    frames,
                    channels,
                    sample_rate: 44100.0,
                })
            }
        }
    }

    async fn write_buffer(&self, id: BufferId, path: &Path) -> Result<(), Self::Error> {
        let path_str = path.to_str().ok_or_else(|| {
            ScsynthError::Io(io::Error::new(io::ErrorKind::InvalidInput, "Invalid path"))
        })?;

        // Send write command (/b_write bufnum path headerFormat sampleFormat numFrames startFrame leaveOpen)
        // Use WAV format with float samples
        self.send_msg(
            "/b_write",
            vec![
                OscType::Int(id.0 as i32),
                OscType::String(path_str.to_string()),
                OscType::String("wav".to_string()), // header format
                OscType::String("float".to_string()), // sample format
                OscType::Int(-1),                   // num frames (-1 = all)
                OscType::Int(0),                    // start frame
                OscType::Int(0),                    // leave open (0 = close after write)
            ],
        )?;

        tracing::debug!("Writing buffer {} to {:?}", id.0, path);
        Ok(())
    }

    async fn free_buffer(&self, id: BufferId) -> Result<(), Self::Error> {
        self.send_msg("/b_free", vec![OscType::Int(id.0 as i32)])?;
        Ok(())
    }

    fn current_time(&self) -> Instant {
        Instant::now()
    }

    async fn sync(&self) -> Result<(), Self::Error> {
        ScsynthBackend::sync(self).await
    }
}

// Helper trait for AddAction conversion to scsynth integer
trait AddActionExt {
    fn to_sc_int(&self) -> i32;
}

impl AddActionExt for AddAction {
    fn to_sc_int(&self) -> i32 {
        match self {
            AddAction::Head => 0,
            AddAction::Tail => 1,
            AddAction::Before => 2,
            AddAction::After => 3,
            AddAction::Replace => 4,
        }
    }
}

/// Set up node tracking for a scsynth backend.
///
/// This registers a callback on the backend that removes ended nodes from
/// voice state when `/n_end` messages are received. This prevents the voice
/// handler from trying to free nodes that have already freed themselves
/// (e.g., via doneAction=2), which causes "node not found" errors.
///
/// # Example
///
/// ```ignore
/// let backend = ScsynthBackend::connect("127.0.0.1:57110").await?;
/// let runtime = Runtime::new(backend);
///
/// // Set up node tracking after creating the runtime
/// setup_node_tracking(runtime.backend(), runtime.state().clone());
/// ```
pub fn setup_node_tracking(
    backend: &ScsynthBackend,
    state: Arc<tokio::sync::RwLock<crate::State>>,
) {
    let state_clone = state;

    backend.on_response(Arc::new(move |response| {
        if let OscResponse::NodeEnd { node_id, .. } = response {
            // Use try_write to avoid blocking in the callback
            // If we can't get the lock, we skip this update (node removal is best-effort)
            let result = state_clone.try_write();
            if let Ok(mut guard) = result {
                // Remove the node from any voice's active_nodes list
                for voice in guard.voices.values_mut() {
                    voice.active_nodes.retain(|&n| n != node_id);
                    voice.note_nodes.retain(|_, &mut n| n != node_id);
                }
                tracing::trace!("Node {} ended, removed from voice tracking", node_id.0);
            }
        }
    }));

    tracing::debug!("Node tracking callback registered on scsynth backend");
}

/// Set up metering for a scsynth backend.
///
/// This registers a callback on the backend that updates meter_levels in the
/// provided state when SendTrig messages are received from link synths.
///
/// The system_link_audio synthdef sends SendTrig messages at ~20Hz with meter data:
/// - trig_id 0: peak left
/// - trig_id 1: peak right
/// - trig_id 2: rms left
/// - trig_id 3: rms right
///
/// # Example
///
/// ```ignore
/// let backend = ScsynthBackend::connect("127.0.0.1:57110").await?;
/// let runtime = Runtime::new(backend);
///
/// // Set up metering after creating the runtime
/// setup_metering(runtime.backend(), runtime.state().clone());
/// ```
pub fn setup_metering(backend: &ScsynthBackend, state: Arc<tokio::sync::RwLock<crate::State>>) {
    let state_clone = state;

    backend.on_response(Arc::new(move |response| {
        if let OscResponse::Trigger {
            node_id,
            trig_id,
            value,
        } = response
        {
            // Only handle meter triggers (IDs 0-3)
            // Higher IDs (100+) are used for other purposes like MIDI triggers
            if (0..=3).contains(&trig_id) {
                // Use try_write to avoid blocking in the callback
                // If we can't get the lock, we skip this update (meters update frequently)
                let result = state_clone.try_write();
                if let Ok(mut guard) = result {
                    let meter = guard.meter_levels.entry(node_id).or_default();
                    match trig_id {
                        0 => meter.peak_left = value,
                        1 => meter.peak_right = value,
                        2 => meter.rms_left = value,
                        3 => {
                            meter.rms_right = value;
                            // Update timestamp only after all 4 values are received
                            // (rms_right is last in the SendTrig sequence)
                            meter.last_update = Some(std::time::Instant::now());
                        }
                        _ => {}
                    }
                }
            }
        }
    }));

    tracing::debug!("Metering callback registered on scsynth backend");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A node of the audio fabric: `(node id, def, read bus, write bus)`.
    fn fabric_node(node: i32, def: &str, read: f32, write: f32) -> TreeSynth {
        let (read_name, write_name) = if def.starts_with("system_link_audio") {
            ("inbus", "outbus")
        } else if def.starts_with("port_to_group_link") {
            ("in_bus", "out_bus")
        } else {
            ("__fx_bus_in", "__fx_bus_out")
        };
        let mut controls = HashMap::new();
        controls.insert(read_name.to_string(), read);
        controls.insert(write_name.to_string(), write);
        TreeSynth {
            node,
            def: def.to_string(),
            controls,
        }
    }

    /// The board's own node tree, transcribed from `/g_dumpTree` while the
    /// instrument played correctly on both sides. Chords chain (bus 10) and
    /// notes chain (bus 23) both fold into the master bus 17 ahead of the
    /// master link that carries 17 to the hardware.
    fn healthy_board_tree() -> Vec<TreeSynth> {
        vec![
            fabric_node(1014, "system_link_audio", 4.0, 0.0),
            fabric_node(1711, "port_to_group_link_2", 12.0, 10.0),
            fabric_node(1321, "saturator", 10.0, 10.0),
            fabric_node(1009, "system_link_audio", 10.0, 17.0),
            fabric_node(1712, "port_to_group_link_2", 19.0, 23.0),
            fabric_node(1001, "ladder_filter", 23.0, 23.0),
            fabric_node(1710, "system_link_audio", 23.0, 17.0),
            fabric_node(1330, "analog_delay", 17.0, 17.0),
            fabric_node(1002, "sg_vca", 17.0, 17.0),
            fabric_node(1004, "limiter", 17.0, 17.0),
            fabric_node(1720, "system_link_audio", 17.0, 0.0),
        ]
    }

    #[test]
    fn a_healthy_tree_reports_no_break() {
        assert_eq!(audio_path_breaks(&healthy_board_tree()), vec![]);
    }

    /// The board defect: a reload recreated the notes group and Tail-added it
    /// to its parent, so the notes chain ended up AFTER the master link synth
    /// that reads bus 17. Measured on the instrument as notes 0.000 with every
    /// SynthDef loaded and the voice spawning happily to its own bus.
    #[test]
    fn a_chain_moved_past_its_consumer_is_reported_with_both_ends_named() {
        let mut tree = healthy_board_tree();
        let notes_chain: Vec<TreeSynth> = tree.drain(4..7).collect();
        tree.extend(notes_chain); // notes chain now runs last, after node 1720

        let breaks = audio_path_breaks(&tree);
        assert_eq!(
            breaks,
            vec![AudioPathBreak {
                node: 1710,
                def: "system_link_audio".to_string(),
                bus: 17,
                consumer_node: 1720,
                consumer_def: "system_link_audio".to_string(),
            }],
            "the stranded link synth and the consumer that already ran must both be named"
        );
    }

    /// Bus 0 is the hardware sink: read by nothing in the tree, written by
    /// every top-level link. Reporting those would drown the real break.
    #[test]
    fn writes_to_a_bus_nothing_ever_reads_are_hardware_sinks_not_breaks() {
        let tree = vec![
            fabric_node(1, "system_link_audio", 4.0, 0.0),
            fabric_node(2, "system_link_audio", 6.0, 0.0),
        ];
        assert_eq!(audio_path_breaks(&tree), vec![]);
    }

    /// An fx insert reads and writes the same bus at the same position. Its
    /// own read must not count as consuming its own write, or every last
    /// effect in every chain would report as broken — but an insert that has
    /// slipped past the node reading its bus still has to be caught.
    #[test]
    fn an_in_place_insert_does_not_consume_its_own_write() {
        let healthy = vec![
            fabric_node(1, "port_to_group_link_2", 8.0, 10.0),
            fabric_node(2, "system_link_audio", 10.0, 17.0),
            fabric_node(3, "reverb", 17.0, 17.0),
            fabric_node(4, "system_link_audio", 17.0, 0.0),
        ];
        assert_eq!(
            audio_path_breaks(&healthy),
            vec![],
            "the last insert in a chain writes for the link synth after it"
        );

        let mut stranded = healthy.clone();
        stranded.swap(2, 3); // reverb now runs after the master link
        let breaks = audio_path_breaks(&stranded);
        assert_eq!(breaks.len(), 1, "{:?}", breaks);
        assert_eq!((breaks[0].node, breaks[0].bus), (3, 17));
    }

    #[test]
    fn a_kr_control_bus_is_never_mistaken_for_an_audio_bus() {
        // A modulator voice writing control bus 17 and a summer reading it:
        // same numbers as the audio fabric above, different namespace. Voices
        // are not fabric, so neither shows up at all.
        let mut controls = HashMap::new();
        controls.insert("out".to_string(), 17.0);
        let tree = vec![
            TreeSynth {
                node: 1,
                def: "lfo_sine".to_string(),
                controls,
            },
            fabric_node(2, "system_link_audio", 4.0, 0.0),
        ];
        assert_eq!(audio_path_breaks(&tree), vec![]);
    }

    #[test]
    fn parse_node_tree_reads_a_group_with_two_synths() {
        // flag, root, 1 child | group 100 with 2 children | synth, 1 control |
        // synth, 2 controls
        let args = vec![
            OscType::Int(1),
            OscType::Int(0),
            OscType::Int(1),
            OscType::Int(100),
            OscType::Int(2),
            OscType::Int(101),
            OscType::Int(-1),
            OscType::String("saw".to_string()),
            OscType::Int(1),
            OscType::String("out".to_string()),
            OscType::Float(12.0),
            OscType::Int(102),
            OscType::Int(-1),
            OscType::String("system_link_audio".to_string()),
            OscType::Int(2),
            OscType::String("inbus".to_string()),
            OscType::Float(12.0),
            OscType::String("outbus".to_string()),
            OscType::Float(0.0),
        ];
        let tree = ScsynthBackend::parse_node_tree(&args).expect("well-formed reply");
        assert_eq!(tree.len(), 2, "groups contribute no entry");
        assert_eq!(tree[0].def, "saw");
        assert_eq!(tree[0].controls.get("out"), Some(&12.0));
        assert_eq!(tree[1].node, 102);
        assert_eq!(tree[1].controls.get("inbus"), Some(&12.0));
    }

    #[test]
    fn parse_node_tree_drops_a_reply_without_control_values() {
        // flag 0: no bus numbers, so nothing can be concluded from it.
        let args = vec![OscType::Int(0), OscType::Int(0), OscType::Int(0)];
        assert_eq!(ScsynthBackend::parse_node_tree(&args), None);
    }

    #[test]
    fn parse_node_tree_drops_a_truncated_reply() {
        // A synth header with its controls cut off — half a tree must not be
        // reasoned about.
        let args = vec![
            OscType::Int(1),
            OscType::Int(0),
            OscType::Int(1),
            OscType::Int(101),
            OscType::Int(-1),
            OscType::String("saw".to_string()),
            OscType::Int(2),
            OscType::String("out".to_string()),
            OscType::Float(12.0),
        ];
        assert_eq!(ScsynthBackend::parse_node_tree(&args), None);
    }

    /// A control mapped to a bus reports as a symbol, not a float. It must
    /// not abort the parse — the rest of the tree is still readable.
    #[test]
    fn parse_node_tree_keeps_going_past_a_mapped_control() {
        let args = vec![
            OscType::Int(1),
            OscType::Int(0),
            OscType::Int(1),
            OscType::Int(101),
            OscType::Int(-1),
            OscType::String("system_link_audio".to_string()),
            OscType::Int(2),
            OscType::String("inbus".to_string()),
            OscType::String("c4".to_string()),
            OscType::String("outbus".to_string()),
            OscType::Float(0.0),
        ];
        let tree = ScsynthBackend::parse_node_tree(&args).expect("well-formed reply");
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].controls.get("inbus"), None);
        assert_eq!(tree[0].controls.get("outbus"), Some(&0.0));
    }

    #[test]
    fn test_add_action_to_sc_int() {
        assert_eq!(AddAction::Head.to_sc_int(), 0);
        assert_eq!(AddAction::Tail.to_sc_int(), 1);
        assert_eq!(AddAction::Before.to_sc_int(), 2);
        assert_eq!(AddAction::After.to_sc_int(), 3);
        assert_eq!(AddAction::Replace.to_sc_int(), 4);
    }

    #[test]
    fn test_scsynth_error_display() {
        let err = ScsynthError::ConnectionFailed("test".to_string());
        assert_eq!(format!("{}", err), "Connection failed: test");

        let err = ScsynthError::ServerNotReady;
        assert_eq!(format!("{}", err), "Server not ready");

        let err = ScsynthError::Timeout;
        assert_eq!(format!("{}", err), "Timeout waiting for response");
    }

    #[test]
    fn test_osc_response_debug() {
        let response = OscResponse::Status {
            num_ugens: 10,
            num_synths: 5,
            num_groups: 2,
            num_synthdefs: 100,
            avg_cpu: 5.5,
            peak_cpu: 10.0,
            sample_rate: 44100.0,
            actual_sample_rate: 44100.0,
        };
        let debug_str = format!("{:?}", response);
        assert!(debug_str.contains("Status"));
    }

    // =========================================================================
    // Bundle time-tag scheduling
    // =========================================================================

    #[test]
    fn schedule_time_tag_past_deadline_falls_back_to_immediate() {
        let now_mono = Instant::now();
        let now_wall = SystemTime::now();

        // A deadline in the past must yield no time tag (send immediately).
        let past = now_mono - Duration::from_millis(10);
        assert_eq!(
            ScsynthBackend::schedule_time_tag(past, now_mono, now_wall),
            None
        );

        // A deadline exactly at "now" is not strictly in the future either.
        assert_eq!(
            ScsynthBackend::schedule_time_tag(now_mono, now_mono, now_wall),
            None
        );
    }

    #[test]
    fn schedule_time_tag_round_trips_through_ntp() {
        let now_mono = Instant::now();
        let now_wall = SystemTime::now();
        let lead = Duration::from_millis(40);

        let tag = ScsynthBackend::schedule_time_tag(now_mono + lead, now_mono, now_wall)
            .expect("future deadline must produce a time tag");

        // Converting the NTP tag back to wall time must land on
        // now_wall + lead (within the ~sub-nanosecond fractional rounding
        // of the 32.32 fixed-point format; allow 1µs).
        let round_tripped: SystemTime = tag.into();
        let expected = now_wall + lead;
        let error = match round_tripped.duration_since(expected) {
            Ok(d) => d,
            Err(e) => e.duration(),
        };
        assert!(
            error < Duration::from_micros(1),
            "round-trip error too large: {:?}",
            error
        );
    }

    #[test]
    fn schedule_time_tag_is_monotonic() {
        let now_mono = Instant::now();
        let now_wall = SystemTime::now();

        let mut previous: Option<(u32, u32)> = None;
        for ms in [1u64, 2, 5, 10, 25, 50, 100, 1000] {
            let tag = ScsynthBackend::schedule_time_tag(
                now_mono + Duration::from_millis(ms),
                now_mono,
                now_wall,
            )
            .expect("future deadline must produce a time tag");
            let key = (tag.seconds, tag.fractional);
            if let Some(prev) = previous {
                assert!(
                    key > prev,
                    "time tags must increase with the deadline: {:?} !> {:?}",
                    key,
                    prev
                );
            }
            previous = Some(key);
        }
    }

    #[test]
    fn encode_bundle_embeds_time_tag_and_messages_in_order() {
        let timetag = OscTime::from((3_913_056_000u32, 0x8000_0000u32));
        let msgs = vec![
            OscMessage {
                addr: "/s_new".to_string(),
                args: vec![
                    OscType::String("kick".to_string()),
                    OscType::Int(2001),
                    OscType::Int(1),
                    OscType::Int(100),
                ],
            },
            OscMessage {
                addr: "/n_map".to_string(),
                args: vec![
                    OscType::Int(2001),
                    OscType::String("cutoff".to_string()),
                    OscType::Int(7),
                ],
            },
        ];

        let bytes = ScsynthBackend::encode_bundle(timetag, msgs).expect("encode");

        // The wire bytes must decode back to a bundle carrying the exact
        // time tag with the messages in order.
        let (_, packet) = decoder::decode_udp(&bytes).expect("decode");
        let OscPacket::Bundle(bundle) = packet else {
            panic!("expected an OSC bundle on the wire");
        };
        assert_eq!(bundle.timetag, timetag);
        assert_eq!(bundle.content.len(), 2);
        let OscPacket::Message(first) = &bundle.content[0] else {
            panic!("expected message");
        };
        assert_eq!(first.addr, "/s_new");
        assert_eq!(first.args[0], OscType::String("kick".to_string()));
        let OscPacket::Message(second) = &bundle.content[1] else {
            panic!("expected message");
        };
        assert_eq!(second.addr, "/n_map");
        assert_eq!(second.args[1], OscType::String("cutoff".to_string()));

        // Raw header check: "#bundle\0" then the big-endian 32.32 time tag.
        assert_eq!(&bytes[0..8], b"#bundle\0");
        assert_eq!(&bytes[8..12], &3_913_056_000u32.to_be_bytes());
        assert_eq!(&bytes[12..16], &0x8000_0000u32.to_be_bytes());
    }

    #[test]
    fn parses_missing_ugen_from_graphdef_reason() {
        assert_eq!(
            ScsynthBackend::parse_missing_ugen(
                "exception in GraphDef_Recv: UGen 'Changed' not installed."
            ),
            Some("Changed".to_string())
        );
        assert_eq!(ScsynthBackend::parse_missing_ugen("other failure"), None);
    }

    #[test]
    fn osc_diag_summarizes_large_payloads_without_dumping_contents() {
        let args = vec![
            OscType::String("spectraphon_sam_oscillator".to_string()),
            OscType::Blob(vec![0; 65_536]),
            OscType::Int(42),
        ];

        assert_eq!(
            ScsynthBackend::summarize_osc_args(&args),
            "\"spectraphon_sam_oscillator\", blob(65536 bytes), 42"
        );
    }

    #[tokio::test]
    async fn done_d_recv_completes_pending_synthdef_load() {
        let pending_buffer_info = Arc::new(Mutex::new(HashMap::new()));
        let pending_sync = Arc::new(Mutex::new(HashMap::new()));
        let pending_control_bus = Arc::new(Mutex::new(HashMap::new()));
        let pending_done = Arc::new(Mutex::new(HashMap::new()));
        let pending_synthdef_load = Arc::new(Mutex::new(None));
        let callbacks = Arc::new(Mutex::new(Vec::new()));
        let server_ready = Arc::new(AtomicBool::new(false));
        let (tx, rx) = oneshot::channel();

        *pending_synthdef_load.lock().unwrap() = Some(PendingSynthDefLoad {
            name: "accepted".to_string(),
            command: "/d_recv",
            tx,
        });

        ScsynthBackend::handle_message(
            OscMessage {
                addr: "/done".to_string(),
                args: vec![OscType::String("/d_recv".to_string())],
            },
            &pending_buffer_info,
            &pending_sync,
            &pending_control_bus,
            &pending_done,
            &pending_synthdef_load,
            &Arc::new(Mutex::new(None)),
            &callbacks,
            &server_ready,
        );

        assert!(matches!(rx.await.unwrap(), SynthDefLoadReply::Done));
        assert!(pending_synthdef_load.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn fail_d_recv_completes_pending_synthdef_load_with_reason() {
        let pending_buffer_info = Arc::new(Mutex::new(HashMap::new()));
        let pending_sync = Arc::new(Mutex::new(HashMap::new()));
        let pending_control_bus = Arc::new(Mutex::new(HashMap::new()));
        let pending_done = Arc::new(Mutex::new(HashMap::new()));
        let pending_synthdef_load = Arc::new(Mutex::new(None));
        let callbacks = Arc::new(Mutex::new(Vec::new()));
        let server_ready = Arc::new(AtomicBool::new(false));
        let (tx, rx) = oneshot::channel();

        *pending_synthdef_load.lock().unwrap() = Some(PendingSynthDefLoad {
            name: "rejected".to_string(),
            command: "/d_recv",
            tx,
        });

        ScsynthBackend::handle_message(
            OscMessage {
                addr: "/fail".to_string(),
                args: vec![
                    OscType::String("/d_recv".to_string()),
                    OscType::String(
                        "exception in GraphDef_Recv: UGen 'Changed' not installed.".to_string(),
                    ),
                ],
            },
            &pending_buffer_info,
            &pending_sync,
            &pending_control_bus,
            &pending_done,
            &pending_synthdef_load,
            &Arc::new(Mutex::new(None)),
            &callbacks,
            &server_ready,
        );

        let SynthDefLoadReply::Fail { reason } = rx.await.unwrap() else {
            panic!("expected fail reply");
        };
        assert!(reason.contains("Changed"));
        assert!(pending_synthdef_load.lock().unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn large_synthdefs_are_loaded_with_d_load() {
        use std::sync::atomic::AtomicBool;

        let server = UdpSocket::bind("127.0.0.1:0").expect("bind fake scsynth");
        server
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("set timeout");
        let server_addr = server.local_addr().expect("server addr").to_string();

        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let recorded_clone = Arc::clone(&recorded);
        let stop_clone = Arc::clone(&stop);
        let server_thread = thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !stop_clone.load(Ordering::Relaxed) {
                let (size, peer) = match server.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                };
                let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) else {
                    continue;
                };
                fake_scsynth_handle(packet, &server, peer, &recorded_clone);
            }
        });

        let backend = ScsynthBackend::connect(&server_addr)
            .await
            .expect("connect should succeed against fake scsynth");

        let data = vec![42u8; D_RECV_MAX_BYTES + 1];
        backend
            .load_synthdef("large_test", &data)
            .await
            .expect("large synthdef should load via /d_load");

        drop(backend);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().expect("server thread join");

        let log = recorded.lock().expect("recorded lock").clone();
        assert!(
            log.iter()
                .any(|entry| entry == &format!("/d_load(bytes={})", data.len())),
            "large SynthDefs must be sent through /d_load; got: {:?}",
            log
        );
        assert!(
            !log.iter().any(|entry| entry == "/d_recv"),
            "large SynthDefs must not be sent through /d_recv; got: {:?}",
            log
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn alloc_buffer_queries_allocated_buffer_info() {
        use std::sync::atomic::AtomicBool;

        let server = UdpSocket::bind("127.0.0.1:0").expect("bind fake scsynth");
        server
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("set timeout");
        let server_addr = server.local_addr().expect("server addr").to_string();

        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let recorded_clone = Arc::clone(&recorded);
        let stop_clone = Arc::clone(&stop);
        let server_thread = thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !stop_clone.load(Ordering::Relaxed) {
                let (size, peer) = match server.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                };
                let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) else {
                    continue;
                };
                fake_scsynth_handle(packet, &server, peer, &recorded_clone);
            }
        });

        let backend = ScsynthBackend::connect(&server_addr)
            .await
            .expect("connect should succeed against fake scsynth");

        let info = backend
            .alloc_buffer(BufferId::new(3019), 384000, 2)
            .await
            .expect("buffer allocation should query metadata");

        drop(backend);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().expect("server thread join");

        assert_eq!(info.frames, 384000);
        assert_eq!(info.channels, 2);

        let log = recorded.lock().expect("recorded lock").clone();
        let alloc_pos = log
            .iter()
            .position(|entry| entry == "/b_alloc(3019,384000,2)")
            .unwrap_or_else(|| panic!("expected /b_alloc record; got: {:?}", log));
        let zero_pos = log
            .iter()
            .position(|entry| entry == "/b_zero(3019)")
            .unwrap_or_else(|| {
                panic!("expected /b_zero after /b_alloc completion; got: {:?}", log)
            });
        let query_pos = log
            .iter()
            .position(|entry| entry == "/b_query(3019)")
            .unwrap_or_else(|| {
                panic!("expected /b_query after /b_zero completion; got: {:?}", log)
            });
        assert!(
            alloc_pos < zero_pos && zero_pos < query_pos,
            "/b_query must be sent after /b_alloc and /b_zero complete; got: {:?}",
            log
        );
    }

    /// `/b_allocRead` is asynchronous (NRT thread) while `/b_query` answers
    /// immediately on the RT thread: querying before the load's `/done`
    /// lands returns stale metadata (0 frames on a fresh buffer, the
    /// previous contents on a reused buffer ID). `load_buffer` must await
    /// the bufnum-correlated `/done /b_allocRead <id>` before `/b_query`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn load_buffer_awaits_alloc_read_done_before_query() {
        use std::sync::atomic::AtomicBool;

        let server = UdpSocket::bind("127.0.0.1:0").expect("bind fake scsynth");
        server
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("set timeout");
        let server_addr = server.local_addr().expect("server addr").to_string();

        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let recorded_clone = Arc::clone(&recorded);
        let stop_clone = Arc::clone(&stop);
        let server_thread = thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !stop_clone.load(Ordering::Relaxed) {
                let (size, peer) = match server.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                };
                let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) else {
                    continue;
                };
                fake_scsynth_handle(packet, &server, peer, &recorded_clone);
            }
        });

        let backend = ScsynthBackend::connect(&server_addr)
            .await
            .expect("connect should succeed against fake scsynth");

        let info = backend
            .load_buffer(BufferId::new(4021), Path::new("/tmp/kick.wav"))
            .await
            .expect("load_buffer should complete against fake scsynth");

        drop(backend);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().expect("server thread join");

        assert_eq!(info.frames, 384000);

        let log = recorded.lock().expect("recorded lock").clone();
        let alloc_read_pos = log
            .iter()
            .position(|entry| entry == "/b_allocRead(4021)")
            .unwrap_or_else(|| panic!("expected /b_allocRead record; got: {:?}", log));
        let query_pos = log
            .iter()
            .position(|entry| entry == "/b_query(4021)")
            .unwrap_or_else(|| {
                panic!(
                    "expected /b_query after /b_allocRead completion; got: {:?}",
                    log
                )
            });
        assert!(
            alloc_read_pos < query_pos,
            "/b_query must be sent only after /b_allocRead's /done; got: {:?}",
            log
        );
    }

    /// Regression test for the scsynth-zombie-on-reconnect bug.
    ///
    /// scsynth is a separate process that survives `systemctl restart
    /// vibelang.service`. Before this fix, `ScsynthBackend::connect` only
    /// sent `/notify` and `/status` — leaving the entire synth tree from
    /// the prior vibelang PID running. New voices then summed onto
    /// hardware buses alongside zombie link/voice synths and produced
    /// asymmetric audio (R-only stereo, mono silent) until host reboot.
    ///
    /// The fix: connect must also send `/g_freeAll [0]` (free everything
    /// under root group 0), `/clearSched` (drop pending bundles), and a
    /// `/sync` ack so the cleanup completes before later commands.
    ///
    /// This test stands up a fake-scsynth UDP server that records every
    /// inbound OSC message, replies to `/status` so `wait_for_server`
    /// returns, and replies to `/sync` so the cleanup ack lands. The
    /// assertion is that the cleanup messages arrived in the right order
    /// (free + clear before the sync that gates further work).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scsynth_reconnect_clears_orphan_synths() {
        use std::sync::atomic::AtomicBool;

        let server = UdpSocket::bind("127.0.0.1:0").expect("bind fake scsynth");
        server
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("set timeout");
        let server_addr = server.local_addr().expect("server addr").to_string();

        let recorded: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let recorded_clone = Arc::clone(&recorded);
        let stop_clone = Arc::clone(&stop);
        let server_thread = thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !stop_clone.load(Ordering::Relaxed) {
                let (size, peer) = match server.recv_from(&mut buf) {
                    Ok(x) => x,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(ref e) if e.kind() == io::ErrorKind::TimedOut => continue,
                    Err(_) => break,
                };
                let Ok((_, packet)) = decoder::decode_udp(&buf[..size]) else {
                    continue;
                };
                fake_scsynth_handle(packet, &server, peer, &recorded_clone);
            }
        });

        let backend = ScsynthBackend::connect(&server_addr)
            .await
            .expect("connect should succeed against fake scsynth");

        // Drop backend to halt its listener thread before we join the
        // fake-server thread (drop signals running=false on the backend).
        drop(backend);
        stop.store(true, Ordering::Relaxed);
        server_thread.join().expect("server thread join");

        let log = recorded.lock().expect("recorded lock").clone();
        let g_free_pos = log
            .iter()
            .position(|s| s == "/g_freeAll(0)")
            .unwrap_or_else(|| panic!("connect must send /g_freeAll [0] — got: {:?}", log));
        let clear_sched_pos = log
            .iter()
            .position(|s| s == "/clearSched")
            .unwrap_or_else(|| panic!("connect must send /clearSched — got: {:?}", log));
        let sync_pos = log
            .iter()
            .position(|s| s.starts_with("/sync("))
            .unwrap_or_else(|| panic!("connect must send /sync — got: {:?}", log));

        assert!(
            g_free_pos < sync_pos,
            "/g_freeAll must precede /sync — got: {:?}",
            log
        );
        assert!(
            clear_sched_pos < sync_pos,
            "/clearSched must precede /sync — got: {:?}",
            log
        );
    }

    /// Fake-scsynth packet handler. Records the path (with a short summary
    /// of the leading int arg for `/g_freeAll` / `/sync`), and answers
    /// `/status` with `/status.reply` plus `/sync N` with `/synced N` so
    /// `ScsynthBackend::connect` makes forward progress.
    fn fake_scsynth_handle(
        packet: OscPacket,
        server: &UdpSocket,
        peer: std::net::SocketAddr,
        recorded: &Arc<Mutex<Vec<String>>>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                let summary = match msg.addr.as_str() {
                    "/g_freeAll" => {
                        let arg = msg.args.first().and_then(|v| {
                            if let OscType::Int(i) = v {
                                Some(*i)
                            } else {
                                None
                            }
                        });
                        format!("/g_freeAll({})", arg.unwrap_or(i32::MIN))
                    }
                    "/sync" => {
                        let id = msg.args.first().and_then(|v| {
                            if let OscType::Int(i) = v {
                                Some(*i)
                            } else {
                                None
                            }
                        });
                        if let Some(id) = id {
                            let reply = OscPacket::Message(OscMessage {
                                addr: "/synced".to_string(),
                                args: vec![OscType::Int(id)],
                            });
                            if let Ok(buf) = encoder::encode(&reply) {
                                let _ = server.send_to(&buf, peer);
                            }
                        }
                        format!("/sync({})", id.unwrap_or(i32::MIN))
                    }
                    "/status" => {
                        let reply = OscPacket::Message(OscMessage {
                            addr: "/status.reply".to_string(),
                            args: vec![
                                OscType::Int(1),
                                OscType::Int(0),
                                OscType::Int(0),
                                OscType::Int(1),
                                OscType::Int(0),
                                OscType::Float(0.0),
                                OscType::Float(0.0),
                                OscType::Double(44100.0),
                                OscType::Double(44100.0),
                            ],
                        });
                        if let Ok(buf) = encoder::encode(&reply) {
                            let _ = server.send_to(&buf, peer);
                        }
                        "/status".to_string()
                    }
                    "/d_load" => {
                        let path = msg.args.first().and_then(|v| {
                            if let OscType::String(path) = v {
                                Some(path)
                            } else {
                                None
                            }
                        });
                        let bytes = path
                            .and_then(|path| fs::read(path).ok())
                            .map(|bytes| bytes.len())
                            .unwrap_or(0);
                        let reply = OscPacket::Message(OscMessage {
                            addr: "/done".to_string(),
                            args: vec![OscType::String("/d_load".to_string())],
                        });
                        if let Ok(buf) = encoder::encode(&reply) {
                            let _ = server.send_to(&buf, peer);
                        }
                        format!("/d_load(bytes={})", bytes)
                    }
                    "/b_alloc" => {
                        let id = msg.args.first().and_then(|v| {
                            if let OscType::Int(id) = v {
                                Some(*id)
                            } else {
                                None
                            }
                        });
                        let frames = msg.args.get(1).and_then(|v| {
                            if let OscType::Int(frames) = v {
                                Some(*frames)
                            } else {
                                None
                            }
                        });
                        let channels = msg.args.get(2).and_then(|v| {
                            if let OscType::Int(channels) = v {
                                Some(*channels)
                            } else {
                                None
                            }
                        });
                        let reply = OscPacket::Message(OscMessage {
                            addr: "/done".to_string(),
                            args: vec![OscType::String("/b_alloc".to_string())],
                        });
                        if let Ok(buf) = encoder::encode(&reply) {
                            let _ = server.send_to(&buf, peer);
                        }
                        format!(
                            "/b_alloc({},{},{})",
                            id.unwrap_or(i32::MIN),
                            frames.unwrap_or(i32::MIN),
                            channels.unwrap_or(i32::MIN)
                        )
                    }
                    "/b_zero" => {
                        let id = msg.args.first().and_then(|v| {
                            if let OscType::Int(id) = v {
                                Some(*id)
                            } else {
                                None
                            }
                        });
                        let reply = OscPacket::Message(OscMessage {
                            addr: "/done".to_string(),
                            args: vec![OscType::String("/b_zero".to_string())],
                        });
                        if let Ok(buf) = encoder::encode(&reply) {
                            let _ = server.send_to(&buf, peer);
                        }
                        format!("/b_zero({})", id.unwrap_or(i32::MIN))
                    }
                    "/b_allocRead" => {
                        // Real scsynth echoes the bufnum in the /done reply
                        // (asynchronous buffer command) — the backend
                        // correlates its waiter on it.
                        let id = msg.args.first().and_then(|v| {
                            if let OscType::Int(id) = v {
                                Some(*id)
                            } else {
                                None
                            }
                        });
                        if let Some(id) = id {
                            let reply = OscPacket::Message(OscMessage {
                                addr: "/done".to_string(),
                                args: vec![
                                    OscType::String("/b_allocRead".to_string()),
                                    OscType::Int(id),
                                ],
                            });
                            if let Ok(buf) = encoder::encode(&reply) {
                                let _ = server.send_to(&buf, peer);
                            }
                        }
                        format!("/b_allocRead({})", id.unwrap_or(i32::MIN))
                    }
                    "/b_query" => {
                        let id = msg.args.first().and_then(|v| {
                            if let OscType::Int(id) = v {
                                Some(*id)
                            } else {
                                None
                            }
                        });
                        if let Some(id) = id {
                            let reply = OscPacket::Message(OscMessage {
                                addr: "/b_info".to_string(),
                                args: vec![
                                    OscType::Int(id),
                                    OscType::Int(384000),
                                    OscType::Int(2),
                                    OscType::Float(48000.0),
                                ],
                            });
                            if let Ok(buf) = encoder::encode(&reply) {
                                let _ = server.send_to(&buf, peer);
                            }
                        }
                        format!("/b_query({})", id.unwrap_or(i32::MIN))
                    }
                    other => other.to_string(),
                };
                if let Ok(mut log) = recorded.lock() {
                    log.push(summary);
                }
            }
            OscPacket::Bundle(bundle) => {
                for content in bundle.content {
                    fake_scsynth_handle(content, server, peer, recorded);
                }
            }
        }
    }
}
