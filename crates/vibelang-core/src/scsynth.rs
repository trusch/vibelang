//! High-level API for the SuperCollider synthesis server (scsynth).
//!
//! This module provides a convenient interface for common scsynth operations:
//!
//! - Loading SynthDefs
//! - Creating and controlling synth nodes
//! - Managing groups for audio routing
//! - Managing audio buffers
//! - VST plugin integration

use anyhow::{anyhow, Result};
use rosc::OscType;
use std::fs;
use std::path::Path;

use crate::osc::OscClient;

/// Action for adding nodes to the node tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddAction {
    /// Add to head of target group (first to execute).
    AddToHead = 0,
    /// Add to tail of target group (last to execute).
    AddToTail = 1,
    /// Add immediately before target node.
    AddBefore = 2,
    /// Add immediately after target node.
    AddAfter = 3,
    /// Replace target node.
    AddReplace = 4,
}

impl From<AddAction> for i32 {
    fn from(action: AddAction) -> Self {
        action as i32
    }
}

/// Node ID for synth and group nodes.
///
/// Use `NodeId::auto()` for server-assigned IDs,
/// or `NodeId::new(id)` for explicit IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub i32);

impl NodeId {
    /// Create a new NodeId with an explicit value.
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    /// Create an auto-assigned NodeId (-1).
    pub fn auto() -> Self {
        Self(-1)
    }

    /// Create the root node ID (0).
    pub fn root() -> Self {
        Self(0)
    }

    /// Get the inner i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }
}

impl From<i32> for NodeId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

impl From<NodeId> for i32 {
    fn from(node_id: NodeId) -> Self {
        node_id.0
    }
}

/// Buffer number for audio buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufNum(pub i32);

impl BufNum {
    /// Create a new buffer number.
    pub fn new(num: i32) -> Self {
        Self(num)
    }

    /// Get the inner i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }
}

impl From<i32> for BufNum {
    fn from(num: i32) -> Self {
        Self(num)
    }
}

impl From<BufNum> for i32 {
    fn from(bufnum: BufNum) -> Self {
        bufnum.0
    }
}

/// Target node for node tree operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target(pub i32);

impl Target {
    /// Create a new target.
    pub fn new(node_id: i32) -> Self {
        Self(node_id)
    }

    /// Create the root target (0).
    pub fn root() -> Self {
        Self(0)
    }

    /// Get the inner i32 value.
    pub fn as_i32(self) -> i32 {
        self.0
    }
}

impl From<i32> for Target {
    fn from(node_id: i32) -> Self {
        Self(node_id)
    }
}

impl From<NodeId> for Target {
    fn from(node_id: NodeId) -> Self {
        Self(node_id.0)
    }
}

impl From<Target> for i32 {
    fn from(target: Target) -> Self {
        target.0
    }
}

/// High-level client for interacting with scsynth.
///
/// Wraps an OSC client and provides methods for common operations
/// like creating synths, loading synthdefs, and managing buffers.
#[derive(Clone)]
pub struct Scsynth {
    /// The underlying OSC client.
    pub osc: OscClient,
}

impl std::fmt::Debug for Scsynth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scsynth")
            .field("addr", &self.osc.addr)
            .finish_non_exhaustive()
    }
}

impl Scsynth {
    /// Connect to scsynth at the given address.
    ///
    /// Automatically enables notifications for node lifecycle events.
    ///
    /// # Arguments
    /// * `addr` - Server address in "host:port" format (e.g., "127.0.0.1:57110")
    pub fn new(addr: &str) -> Result<Self> {
        let osc = OscClient::new(addr)?;
        osc.send_msg("/notify", vec![OscType::Int(1)])?;
        Ok(Self { osc })
    }

    /// Create a no-op Scsynth for validation mode.
    ///
    /// All operations will succeed but do nothing.
    /// This is useful for validating scripts without a running SuperCollider server.
    pub fn noop() -> Self {
        Self {
            osc: OscClient::noop(),
        }
    }

    /// Check if this client is in noop mode.
    pub fn is_noop(&self) -> bool {
        self.osc.is_noop()
    }

    /// Load a SynthDef from raw bytes.
    pub fn d_recv_bytes(&self, bytes: Vec<u8>) -> Result<()> {
        self.osc.send_msg("/d_recv", vec![OscType::Blob(bytes)])?;
        Ok(())
    }

    /// Load a SynthDef from a file.
    pub fn d_recv_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let bytes = fs::read(path)?;
        self.d_recv_bytes(bytes)
    }

    /// Allocate a buffer and read an audio file into it.
    pub fn b_alloc_read<P: AsRef<Path>>(&self, bufnum: BufNum, path: P) -> Result<()> {
        let p = path
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow!("invalid path"))?;
        self.osc.send_msg(
            "/b_allocRead",
            vec![
                OscType::Int(bufnum.as_i32()),
                OscType::String(p.into()),
                OscType::Int(0),  // start frame
                OscType::Int(-1), // num frames (-1 = all)
            ],
        )?;
        Ok(())
    }

    /// Create a new synth node.
    ///
    /// # Arguments
    /// * `def` - SynthDef name
    /// * `node_id` - Node ID (use `NodeId::auto()` for auto-assignment)
    /// * `add_action` - Where to add the node
    /// * `target` - Target node for placement
    /// * `controls` - Initial control values
    pub fn s_new<S: Into<String>>(
        &self,
        def: S,
        node_id: NodeId,
        add_action: AddAction,
        target: Target,
        controls: &[(impl AsRef<str>, f32)],
    ) -> Result<()> {
        let def_name = def.into();
        let mut args: Vec<OscType> = vec![
            OscType::String(def_name.clone()),
            OscType::Int(node_id.as_i32()),
            OscType::Int(add_action.into()),
            OscType::Int(target.as_i32()),
        ];
        for (k, v) in controls {
            args.push(OscType::String(k.as_ref().to_string()));
            args.push(OscType::Float(*v));
        }

        log::debug!(
            "[OSC] /s_new: def='{}', node={}, action={}, target={}, controls={:?}",
            def_name,
            node_id.as_i32(),
            add_action as i32,
            target.as_i32(),
            controls
                .iter()
                .map(|(k, v)| format!("{}={}", k.as_ref(), v))
                .collect::<Vec<_>>()
        );

        self.osc.send_msg("/s_new", args)?;
        Ok(())
    }

    /// Set control values on an existing node.
    pub fn n_set(&self, node_id: NodeId, controls: &[(impl AsRef<str>, f32)]) -> Result<()> {
        let mut args: Vec<OscType> = vec![OscType::Int(node_id.as_i32())];
        for (k, v) in controls {
            args.push(OscType::String(k.as_ref().to_string()));
            args.push(OscType::Float(*v));
        }
        self.osc.send_msg("/n_set", args)?;
        Ok(())
    }

    /// Free (stop and remove) a node.
    pub fn n_free(&self, node_id: NodeId) -> Result<()> {
        self.osc
            .send_msg("/n_free", vec![OscType::Int(node_id.as_i32())])?;
        Ok(())
    }

    /// Pause or resume a node.
    pub fn n_run(&self, node_id: NodeId, run: bool) -> Result<()> {
        self.osc.send_msg(
            "/n_run",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(if run { 1 } else { 0 }),
            ],
        )?;
        Ok(())
    }

    /// Create a new group node.
    pub fn g_new(&self, node_id: NodeId, add_action: AddAction, target: Target) -> Result<()> {
        self.g_new_with_run(node_id, add_action, target, true)
    }

    /// Create a new group node with explicit run state.
    pub fn g_new_with_run(
        &self,
        node_id: NodeId,
        add_action: AddAction,
        target: Target,
        run: bool,
    ) -> Result<()> {
        self.osc.send_msg(
            "/g_new",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(add_action.into()),
                OscType::Int(target.as_i32()),
                OscType::Int(if run { 1 } else { 0 }),
            ],
        )?;
        Ok(())
    }

    /// Free all nodes in a group (and subgroups).
    pub fn g_free_all(&self, node_id: i32) -> Result<()> {
        self.osc
            .send_msg("/g_freeAll", vec![OscType::Int(node_id)])?;
        Ok(())
    }

    /// Allocate an empty buffer.
    pub fn b_alloc(&self, bufnum: BufNum, num_frames: i32, num_channels: i32) -> Result<()> {
        self.osc.send_msg(
            "/b_alloc",
            vec![
                OscType::Int(bufnum.as_i32()),
                OscType::Int(num_frames),
                OscType::Int(num_channels),
            ],
        )?;
        Ok(())
    }

    /// Write buffer contents to a file.
    #[allow(clippy::too_many_arguments)]
    pub fn b_write<P: AsRef<Path>>(
        &self,
        bufnum: BufNum,
        path: P,
        header_format: &str,
        sample_format: &str,
        start_frame: i32,
        num_frames: i32,
        leave_open: bool,
    ) -> Result<()> {
        let p = path
            .as_ref()
            .to_str()
            .ok_or_else(|| anyhow!("invalid path"))?;
        self.osc.send_msg(
            "/b_write",
            vec![
                OscType::Int(bufnum.as_i32()),
                OscType::String(p.into()),
                OscType::String(header_format.to_string()),
                OscType::String(sample_format.to_string()),
                OscType::Int(start_frame),
                OscType::Int(num_frames),
                OscType::Int(if leave_open { 1 } else { 0 }),
            ],
        )?;
        Ok(())
    }

    /// Close a buffer file.
    pub fn b_close(&self, bufnum: BufNum) -> Result<()> {
        self.osc
            .send_msg("/b_close", vec![OscType::Int(bufnum.as_i32())])?;
        Ok(())
    }

    /// Free a buffer.
    pub fn b_free(&self, bufnum: BufNum) -> Result<()> {
        self.osc
            .send_msg("/b_free", vec![OscType::Int(bufnum.as_i32())])?;
        Ok(())
    }

    // === VSTPlugin UGen Commands ===

    /// Open a VST plugin in a VSTPlugin UGen.
    pub fn vst_open(
        &self,
        node_id: NodeId,
        synth_index: i32,
        plugin_path: &str,
        editor: bool,
    ) -> Result<()> {
        log::debug!(
            "[VST] Opening plugin '{}' on node {} (index: {}, editor: {})",
            plugin_path,
            node_id.as_i32(),
            synth_index,
            editor
        );
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/open".to_string()),
                OscType::String(plugin_path.to_string()),
                OscType::Int(if editor { 1 } else { 0 }),
                OscType::Int(0), // threaded
                OscType::Int(0), // mode
            ],
        )?;
        Ok(())
    }

    /// Close a VST plugin.
    pub fn vst_close(&self, node_id: NodeId, synth_index: i32) -> Result<()> {
        log::debug!(
            "[VST] Closing plugin on node {} (index: {})",
            node_id.as_i32(),
            synth_index
        );
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/close".to_string()),
            ],
        )?;
        Ok(())
    }

    /// Send MIDI note-on to a VST plugin.
    pub fn vst_midi_note_on(
        &self,
        node_id: NodeId,
        synth_index: i32,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<()> {
        let status = 0x90 | (channel & 0x0F);
        log::debug!(
            "[VST] MIDI NoteOn: node={}, ch={}, note={}, vel={}",
            node_id.as_i32(),
            channel,
            note,
            velocity
        );
        let midi_bytes = vec![status, note, velocity];
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/midi_msg".to_string()),
                OscType::Blob(midi_bytes),
            ],
        )?;
        Ok(())
    }

    /// Send MIDI note-off to a VST plugin.
    pub fn vst_midi_note_off(
        &self,
        node_id: NodeId,
        synth_index: i32,
        channel: u8,
        note: u8,
        velocity: u8,
    ) -> Result<()> {
        let status = 0x80 | (channel & 0x0F);
        log::debug!(
            "[VST] MIDI NoteOff: node={}, ch={}, note={}, vel={}",
            node_id.as_i32(),
            channel,
            note,
            velocity
        );
        let midi_bytes = vec![status, note, velocity];
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/midi_msg".to_string()),
                OscType::Blob(midi_bytes),
            ],
        )?;
        Ok(())
    }

    /// Set a VST plugin parameter by index.
    pub fn vst_set(
        &self,
        node_id: NodeId,
        synth_index: i32,
        param_index: i32,
        value: f32,
    ) -> Result<()> {
        log::debug!(
            "[VST] Set param: node={}, param={}, value={}",
            node_id.as_i32(),
            param_index,
            value
        );
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/set".to_string()),
                OscType::Int(param_index),
                OscType::Float(value),
            ],
        )?;
        Ok(())
    }

    /// Set a VST plugin parameter by name.
    pub fn vst_set_by_name(
        &self,
        node_id: NodeId,
        synth_index: i32,
        param_name: &str,
        value: f32,
    ) -> Result<()> {
        log::debug!(
            "[VST] Set param by name: node={}, param='{}', value={}",
            node_id.as_i32(),
            param_name,
            value
        );
        self.osc.send_msg(
            "/u_cmd",
            vec![
                OscType::Int(node_id.as_i32()),
                OscType::Int(synth_index),
                OscType::String("/set".to_string()),
                OscType::String(param_name.to_string()),
                OscType::Float(value),
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        assert_eq!(NodeId::auto().as_i32(), -1);
        assert_eq!(NodeId::root().as_i32(), 0);
        assert_eq!(NodeId::new(42).as_i32(), 42);
    }

    #[test]
    fn test_add_action() {
        assert_eq!(AddAction::AddToHead as i32, 0);
        assert_eq!(AddAction::AddToTail as i32, 1);
        assert_eq!(AddAction::AddBefore as i32, 2);
        assert_eq!(AddAction::AddAfter as i32, 3);
        assert_eq!(AddAction::AddReplace as i32, 4);
    }

    #[test]
    fn test_target_conversions() {
        let target = Target::from(NodeId::new(100));
        assert_eq!(target.as_i32(), 100);
    }
}
