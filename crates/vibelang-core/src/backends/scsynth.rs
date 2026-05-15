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
use rosc::{decoder, encoder, OscMessage, OscPacket, OscType};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
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

/// SuperCollider scsynth backend.
///
/// Communicates with scsynth via OSC over UDP.
pub struct ScsynthBackend {
    /// UDP socket for sending OSC messages.
    socket: Arc<UdpSocket>,
    /// Target server address.
    addr: String,
    /// Creation time for relative timing.
    #[allow(dead_code)] // Reserved for bundle timestamping
    start_time: Instant,
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
    /// Next sync ID for /sync messages.
    next_sync_id: Arc<std::sync::atomic::AtomicI32>,
    /// General response callbacks.
    callbacks: Arc<Mutex<Vec<OscCallback>>>,
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
        let next_sync_id = Arc::new(std::sync::atomic::AtomicI32::new(1));
        let callbacks = Arc::new(Mutex::new(Vec::new()));

        let socket = Arc::new(socket);

        let backend = Self {
            socket: socket.clone(),
            addr: addr.to_string(),
            start_time: Instant::now(),
            running: running.clone(),
            node_end_tx,
            pending_buffer_info: pending_buffer_info.clone(),
            pending_sync: pending_sync.clone(),
            pending_control_bus: pending_control_bus.clone(),
            pending_done: pending_done.clone(),
            pending_synthdef_load: pending_synthdef_load.clone(),
            next_sync_id,
            callbacks: callbacks.clone(),
            server_ready: server_ready.clone(),
        };

        // Start OSC listener thread
        Self::start_listener(
            socket.clone(),
            running.clone(),
            pending_buffer_info.clone(),
            pending_sync.clone(),
            pending_control_bus.clone(),
            pending_done.clone(),
            pending_synthdef_load.clone(),
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
        callbacks: &Arc<Mutex<Vec<OscCallback>>>,
        server_ready: &Arc<AtomicBool>,
    ) {
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
                    if let Some(sender) = pending.remove(&command) {
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
        let msg = OscMessage {
            addr: path.to_string(),
            args,
        };
        let packet = OscPacket::Message(msg);
        let buf = encoder::encode(&packet)?;
        self.socket.send(&buf)?;
        Ok(())
    }

    async fn send_msg_and_await_done(
        &self,
        path: &str,
        args: Vec<OscType>,
    ) -> Result<(), ScsynthError> {
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self
                .pending_done
                .lock()
                .map_err(|_| ScsynthError::LockPoisoned)?;
            if pending.insert(path.to_string(), tx).is_some() {
                return Err(ScsynthError::ConnectionFailed(format!(
                    "command already pending: {}",
                    path
                )));
            }
        }

        if let Err(err) = self.send_msg(path, args) {
            if let Ok(mut pending) = self.pending_done.lock() {
                pending.remove(path);
            }
            return Err(err);
        }

        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(format!(
                "{} completion response channel closed",
                path
            ))),
            Err(_) => {
                if let Ok(mut pending) = self.pending_done.lock() {
                    pending.remove(path);
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

        // Wait for response with reasonable timeout.
        // Since MIDI clock is now handled in a dedicated thread, we don't need
        // aggressive timeouts here anymore.
        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(())) => {
                tracing::trace!("Sync {} completed", sync_id);
                Ok(())
            }
            Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
                "Sync response channel closed".to_string(),
            )),
            Err(_) => {
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
        let mut args: Vec<OscType> = vec![
            OscType::String(def.to_string()),
            OscType::Int(node.0 as i32),
            OscType::Int(action.to_sc_int()),
            OscType::Int(target.0 as i32),
        ];

        // Add parameters
        for (key, value) in params {
            args.push(OscType::String(key.clone()));
            args.push(OscType::Float(*value));
        }

        tracing::debug!(
            "s_new: def='{}', node={}, target={}, params={:?}",
            def,
            node.0,
            target.0,
            params
        );

        self.send_msg("/s_new", args)?;
        Ok(())
    }

    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error> {
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
        self.send_msg("/n_free", vec![OscType::Int(node.0 as i32)])?;
        Ok(())
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

        self.send_msg(
            "/b_allocRead",
            vec![
                OscType::Int(id.0 as i32),
                OscType::String(path_str.to_string()),
                OscType::Int(0),  // start frame
                OscType::Int(-1), // num frames (-1 = all)
            ],
        )?;

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
