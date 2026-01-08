//! SuperCollider (scsynth) backend implementation.
//!
//! This module provides a [`Backend`] implementation for SuperCollider's
//! synthesis server (scsynth) using OSC over UDP.
//!
//! # Example
//!
//! ```ignore
//! use vibelang_core2::backends::ScsynthBackend;
//! use vibelang_core2::Runtime;
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
use std::io;
use std::net::UdpSocket;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
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
}

impl std::fmt::Display for ScsynthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScsynthError::Io(e) => write!(f, "IO error: {}", e),
            ScsynthError::Osc(e) => write!(f, "OSC error: {:?}", e),
            ScsynthError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            ScsynthError::ServerNotReady => write!(f, "Server not ready"),
            ScsynthError::Timeout => write!(f, "Timeout waiting for response"),
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
        let callbacks = Arc::new(Mutex::new(Vec::new()));

        let socket = Arc::new(socket);

        let backend = Self {
            socket: socket.clone(),
            addr: addr.to_string(),
            start_time: Instant::now(),
            running: running.clone(),
            node_end_tx,
            pending_buffer_info: pending_buffer_info.clone(),
            callbacks: callbacks.clone(),
            server_ready: server_ready.clone(),
        };

        // Start OSC listener thread
        Self::start_listener(
            socket.clone(),
            running.clone(),
            pending_buffer_info.clone(),
            callbacks.clone(),
            server_ready.clone(),
        );

        // Enable notifications
        backend.send_msg("/notify", vec![OscType::Int(1)])?;

        // Wait for server to be ready
        backend.wait_for_server(Duration::from_secs(5)).await?;

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
        callbacks: &Arc<Mutex<Vec<OscCallback>>>,
        server_ready: &Arc<AtomicBool>,
    ) {
        match packet {
            OscPacket::Message(msg) => {
                Self::handle_message(msg, pending_buffer_info, callbacks, server_ready);
            }
            OscPacket::Bundle(bundle) => {
                for content in bundle.content {
                    Self::handle_packet(content, pending_buffer_info, callbacks, server_ready);
                }
            }
        }
    }

    /// Handle an incoming OSC message.
    fn handle_message(
        msg: OscMessage,
        pending_buffer_info: &Arc<Mutex<HashMap<u32, oneshot::Sender<BufferInfo>>>>,
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
                    let parent_group =
                        NodeId::new(Self::get_int(&msg.args, 1).unwrap_or(0) as u32);
                    let prev_node = NodeId::new(Self::get_int(&msg.args, 2).unwrap_or(-1) as u32);
                    let next_node = NodeId::new(Self::get_int(&msg.args, 3).unwrap_or(-1) as u32);
                    let is_group = Self::get_int(&msg.args, 4).unwrap_or(0) == 1;

                    tracing::debug!("Node {} ended (parent={})", node_id.0, parent_group.0);

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
                    let parent_group =
                        NodeId::new(Self::get_int(&msg.args, 1).unwrap_or(0) as u32);
                    let prev_node = NodeId::new(Self::get_int(&msg.args, 2).unwrap_or(-1) as u32);
                    let next_node = NodeId::new(Self::get_int(&msg.args, 3).unwrap_or(-1) as u32);
                    let is_group = Self::get_int(&msg.args, 4).unwrap_or(0) == 1;

                    tracing::debug!("Node {} started (parent={})", node_id.0, parent_group.0);

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
                Some(OscResponse::Done { command })
            }
            "/fail" => {
                // Command failed
                let command = Self::get_string(&msg.args, 0).unwrap_or_default();
                let reason = Self::get_string(&msg.args, 1).unwrap_or_default();
                tracing::warn!("Fail: {} - {}", command, reason);
                Some(OscResponse::Fail { command, reason })
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
            let mut pending = self.pending_buffer_info.lock().unwrap();
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

    async fn load_synthdef(&self, _name: &str, data: &[u8]) -> Result<(), Self::Error> {
        self.send_msg("/d_recv", vec![OscType::Blob(data.to_vec())])?;
        Ok(())
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

    async fn load_buffer(&self, id: BufferId, path: &Path) -> Result<BufferInfo, Self::Error> {
        let path_str = path.to_str().ok_or_else(|| {
            ScsynthError::Io(io::Error::new(io::ErrorKind::InvalidInput, "Invalid path"))
        })?;

        // Create oneshot channel for buffer info response
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_buffer_info.lock().unwrap();
            pending.insert(id.0, tx);
        }

        // Send allocRead command
        self.send_msg(
            "/b_allocRead",
            vec![
                OscType::Int(id.0 as i32),
                OscType::String(path_str.to_string()),
                OscType::Int(0),  // start frame
                OscType::Int(-1), // num frames (-1 = all)
            ],
        )?;

        // Wait for buffer info response with timeout
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(info)) => {
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
            Ok(Err(_)) => {
                // Channel closed, return placeholder
                tracing::warn!(
                    "Buffer info channel closed for {}, using placeholder values",
                    id.0
                );
                Ok(BufferInfo {
                    frames: 0,
                    channels: 2,
                    sample_rate: 44100.0,
                })
            }
            Err(_) => {
                // Timeout - query buffer info manually
                tracing::warn!(
                    "Buffer {} load timeout, querying info manually",
                    id.0
                );
                self.query_buffer_info(id).await
            }
        }
    }

    async fn alloc_buffer(
        &self,
        id: BufferId,
        frames: u32,
        channels: u16,
    ) -> Result<BufferInfo, Self::Error> {
        // Create oneshot channel for buffer info response
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_buffer_info.lock().unwrap();
            pending.insert(id.0, tx);
        }

        // Send alloc command
        self.send_msg(
            "/b_alloc",
            vec![
                OscType::Int(id.0 as i32),
                OscType::Int(frames as i32),
                OscType::Int(channels as i32),
            ],
        )?;

        // Wait for buffer info response with timeout
        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(info)) => {
                tracing::debug!(
                    "Allocated buffer {} ({} frames, {} channels)",
                    id.0,
                    info.frames,
                    info.channels
                );
                Ok(info)
            }
            Ok(Err(_)) => {
                // Channel closed, return placeholder
                tracing::warn!(
                    "Buffer info channel closed for {}, using allocated values",
                    id.0
                );
                Ok(BufferInfo {
                    frames,
                    channels,
                    sample_rate: 44100.0,
                })
            }
            Err(_) => {
                // Timeout - return the allocated values
                tracing::warn!("Buffer {} alloc timeout, using allocated values", id.0);
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
                OscType::String("wav".to_string()),   // header format
                OscType::String("float".to_string()), // sample format
                OscType::Int(-1),                     // num frames (-1 = all)
                OscType::Int(0),                      // start frame
                OscType::Int(0),                      // leave open (0 = close after write)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
