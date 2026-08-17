//! Groups handler implementation.

use crate::backend::{AddAction, Backend};
use crate::compat::{Duration, Instant, RwLock};
use crate::handlers::voices::{voice_is_gated, voice_release_grace};
use crate::state::{GroupState, State};
use crate::traits::Groups;
use crate::types::{BusId, GroupId, NodeId, ParamMap};
use crate::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

/// Deferred teardown for a deleted group.
///
/// Freeing a group node kills every child node with it — including voice
/// synths that were just gate-released and are still tailing out. Group
/// deletion therefore removes the group from the state immediately but
/// defers the backend frees (and the node-ID/bus reclaim, so the group's
/// audio bus can't be reallocated while released children still write into
/// it) until the caller-provided grace period has elapsed.
struct PendingGroupFree {
    requested_at: Instant,
    grace: Duration,
    /// Link synth node (freed first) and group node, in free order.
    nodes: Vec<NodeId>,
    /// The group's stereo audio bus, returned to the pool at the deadline.
    audio_bus: BusId,
}

/// Handler for group operations.
pub struct GroupsHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
    /// Deferred group frees, drained by [`Self::tick`].
    pending_frees: Arc<RwLock<Vec<PendingGroupFree>>>,
}

/// Compute whether a group should be audibly muted, given the global solo
/// state. Solo mode (any group soloed) silences every group that is not
/// itself soloed; a muted group stays silent even when soloed.
fn effective_mute(muted: bool, soloed: bool, any_soloed: bool) -> bool {
    if any_soloed {
        muted || !soloed
    } else {
        muted
    }
}

impl<B: Backend> GroupsHandler<B> {
    /// Create a new groups handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self {
            backend,
            state,
            pending_frees: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Process deferred group frees whose grace period has elapsed.
    ///
    /// Called by the runtime's tick loop (same idiom as
    /// `EffectsHandler::tick`). Frees the stashed nodes on the backend
    /// first, then recycles the node IDs and returns the group's audio bus
    /// to the allocator — never before, so a released child's audio can't
    /// bleed into a reallocated bus.
    pub async fn tick(&self) {
        let now = Instant::now();
        let due: Vec<PendingGroupFree> = {
            let mut pending = self.pending_frees.write().await;
            if pending.is_empty() {
                return;
            }
            let mut due = Vec::new();
            let mut remaining = Vec::new();
            for entry in pending.drain(..) {
                // On WASM there is no runtime-driven timing guarantee; free
                // immediately like EffectsHandler does.
                let elapsed = cfg!(target_arch = "wasm32")
                    || now.duration_since(entry.requested_at) >= entry.grace;
                if elapsed {
                    due.push(entry);
                } else {
                    remaining.push(entry);
                }
            }
            *pending = remaining;
            due
        };

        for entry in due {
            for node in &entry.nodes {
                tracing::debug!("Group reclaim: grace elapsed, freeing node {:?}", node);
                let _ = self.backend.free_node(*node).await;
            }
            let mut state = self.state.write().await;
            for node in &entry.nodes {
                state.free_node_id(*node);
            }
            state.free_audio_bus(entry.audio_bus, 2);
        }
    }

    /// Delete a group, deferring the backend frees and resource reclaim by
    /// `grace`.
    ///
    /// With `grace == 0` this is the legacy immediate teardown. Reload
    /// passes the maximum release grace of the voices it gate-released in
    /// the same pass, so the group node (and every released child inside
    /// it) survives until the tails have finished — freeing a silent
    /// subtree never clicks.
    pub async fn delete_with_grace(&self, id: GroupId, grace: Duration) -> Result<()> {
        let immediate = grace.is_zero();
        let group = {
            let mut state = self.state.write().await;
            let group = state.groups.remove(&id).ok_or(Error::GroupNotFound(id))?;
            // Drop the live chain-order tracking; a recreated group with the
            // same id starts a fresh chain.
            state.group_effect_chain.remove(&id);
            group
        };

        if immediate {
            // Free the link synth first if it exists
            if let Some(link_node_id) = group.link_synth_node_id {
                let _ = self.backend.free_node(link_node_id).await;
            }

            // Free the group node (this also frees all child nodes)
            self.backend
                .free_node(group.node_id)
                .await
                .map_err(Error::backend)?;

            // Keep IDs and the bus owned until every backend free above has
            // been sent. Reclaiming them before /n_free lets a same-pass
            // create reuse an ID or bus while the old subtree still exists.
            let mut state = self.state.write().await;
            state.free_node_id(group.node_id);
            if let Some(link_id) = group.link_synth_node_id {
                state.free_node_id(link_id);
            }
            state.free_audio_bus(group.audio_bus, 2);

            tracing::debug!("Deleted group {} (node_id={})", id.0, group.node_id.0);
            return Ok(());
        }

        let mut nodes = Vec::with_capacity(2);
        if let Some(link_node_id) = group.link_synth_node_id {
            nodes.push(link_node_id);
        }
        nodes.push(group.node_id);
        tracing::debug!(
            "Deleted group {} (node_id={}); backend free deferred by {:?}",
            id.0,
            group.node_id.0,
            grace
        );
        self.pending_frees.write().await.push(PendingGroupFree {
            requested_at: Instant::now(),
            grace,
            nodes,
            audio_bus: group.audio_bus,
        });
        Ok(())
    }

    /// Finalize all groups by creating link synths for audio routing.
    ///
    /// This creates `system_link_audio` synths that route audio from each
    /// group's audio bus to its parent's bus (or bus 0 for the main output).
    pub async fn finalize(&self) -> Result<()> {
        // Collect groups that need link synths
        #[allow(clippy::type_complexity)]
        let groups_to_link: Vec<(GroupId, NodeId, BusId, BusId, Option<u32>, f32, f32)> = {
            let state = self.state.read().await;

            // Mute/solo state must survive finalize: a group muted (or
            // implicitly muted by another group's solo) before its link synth
            // exists gets the mute passed as an initial /s_new arg.
            let any_soloed = state.groups.values().any(|g| g.soloed);

            state
                .groups
                .values()
                .filter(|g| g.link_synth_node_id.is_none())
                .map(|g| {
                    // Determine output bus:
                    // 1. Explicit output_bus override → route to hardware bus directly
                    // 2. Parent group → route to parent's audio bus
                    // 3. Root group → route to bus 0 (main stereo output)
                    let out_bus = if let Some(hw_bus) = g.output_bus {
                        BusId::new(hw_bus)
                    } else {
                        g.parent
                            .and_then(|p| state.groups.get(&p))
                            .map(|pg| pg.audio_bus)
                            .unwrap_or(BusId::new(0))
                    };
                    let amp = g.params.get("amp").copied().unwrap_or(1.0);
                    let mute = if effective_mute(g.muted, g.soloed, any_soloed) {
                        1.0
                    } else {
                        0.0
                    };

                    (
                        g.id,
                        g.node_id,
                        g.audio_bus,
                        out_bus,
                        g.output_channels,
                        amp,
                        mute,
                    )
                })
                .collect()
        };

        // Create link synths for each group
        let num_groups = groups_to_link.len();
        for (group_id, group_node_id, in_bus, out_bus, output_channels, amp, mute) in groups_to_link
        {
            // Allocate node ID for link synth
            let link_node_id = {
                let mut state = self.state.write().await;
                state.alloc_node_id()?
            };

            // Create the link synth
            // It reads from in_bus and writes to out_bus
            // Note: system_link_audio uses "inbus" and "outbus" parameter names
            let mut params = ParamMap::new();
            params.insert("inbus".to_string(), in_bus.0 as f32);
            params.insert("outbus".to_string(), out_bus.0 as f32);
            params.insert("amp".to_string(), amp);
            params.insert("mute".to_string(), mute);

            // Pick the link-synth variant based on the group's hardware
            // channel count: Some(1) → mono mixdown variant; Some(2) or
            // None (the implicit-stereo default) → the stereo default.
            let link_synth_name = match output_channels {
                Some(1) => "system_link_audio_mono",
                _ => "system_link_audio",
            };

            self.backend
                .create_synth(
                    link_synth_name,
                    link_node_id,
                    group_node_id,
                    AddAction::Tail, // Add at tail of the group
                    &params,
                )
                .await
                .map_err(Error::backend)?;

            self.backend
                .set_param(link_node_id, "amp", amp)
                .await
                .map_err(Error::backend)?;

            // Update state with link synth node ID
            {
                let mut state = self.state.write().await;
                if let Some(group) = state.groups.get_mut(&group_id) {
                    group.link_synth_node_id = Some(link_node_id);
                }
            }

            tracing::info!(
                "Created link synth for group {} (node_id={}, inbus={}, outbus={})",
                group_id.0,
                link_node_id.0,
                in_bus.0,
                out_bus.0
            );
        }

        tracing::info!("Finalized {} groups with link synths", num_groups);
        Ok(())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: Backend> Groups for GroupsHandler<B> {
    async fn create(&self, id: GroupId, name: &str, parent: Option<GroupId>) -> Result<()> {
        let mut state = self.state.write().await;

        if state.groups.contains_key(&id) {
            return Err(Error::GroupExists(id));
        }

        // Allocate node ID and audio bus
        let node_id = state.alloc_node_id()?;
        let audio_bus = state.alloc_bus_id()?;

        // Determine target for placement.
        //
        // A child group feeds the parent's bus through its own link synth, so
        // it has to run BEFORE everything in the parent that reads that bus:
        // the parent's fx chain and the parent's link synth. scsynth evaluates
        // in tree order and zeroes audio buses every block, so a child group
        // Tail-added after the parent's link synth writes a bus nobody reads
        // any more — that whole branch of the instrument goes silent while
        // every synth in it is alive and correct.
        //
        // On a cold build the parent has neither fx nor link yet (both come
        // later, fx at Tail and the link at Tail from `finalize`), so Tail is
        // right and reproduces the cold-boot order. On a RELOAD that recreates
        // a group the parent already carries both, which is where Tail strands
        // it. Same three-case placement as `RoutesHandler::spawn_route` and
        // `EffectsHandler::add`, one slot earlier than the fx chain.
        let (target, action) = if let Some(parent_id) = parent {
            let parent_node_id = state
                .groups
                .get(&parent_id)
                .ok_or(Error::GroupNotFound(parent_id))?
                .node_id;
            let first_fx_node = state.first_effect_node_in_group(parent_id);
            let parent_link = state
                .groups
                .get(&parent_id)
                .and_then(|g| g.link_synth_node_id);
            // Which anchor was available decides everything here, and the
            // wrong branch is inaudible rather than an error — so record what
            // the parent looked like, not just where the child went.
            tracing::debug!(
                "placing group {} under parent {} (node {}): first fx {:?}, link {:?}",
                id.0,
                parent_id.0,
                parent_node_id.0,
                first_fx_node.map(|n| n.0),
                parent_link.map(|n| n.0),
            );
            if let Some(first_fx_node) = first_fx_node {
                (first_fx_node, AddAction::Before)
            } else if let Some(link_node) = parent_link {
                (link_node, AddAction::Before)
            } else {
                (parent_node_id, AddAction::Tail)
            }
        } else {
            // Add to root group (node 0). Root children write either bus 0 or
            // a sibling's bus via their own link; the driver drains bus 0 at
            // the end of the block, so Tail carries no ordering hazard here.
            (NodeId::new(0), AddAction::Tail)
        };

        // Create group in backend
        self.backend
            .create_group(node_id, target, action)
            .await
            .map_err(Error::backend)?;

        // Store state
        state.groups.insert(
            id,
            GroupState {
                id,
                name: name.to_string(),
                parent,
                node_id,
                audio_bus,
                link_synth_node_id: None, // Created later by FinalizeGroups
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        tracing::info!(
            "Created group {} '{}' (node_id={}, audio_bus={}) - voices should output to bus {}",
            id.0,
            name,
            node_id.0,
            audio_bus.0,
            audio_bus.0
        );

        Ok(())
    }

    async fn delete(&self, id: GroupId) -> Result<()> {
        // Direct deletion (GroupMessage::Delete) may hit a group that still
        // contains sounding voice nodes — freeing the group node would
        // truncate them with a click. Gate-release the gated children first
        // and defer the group free past their longest release. Reload never
        // takes this branch (it gracefully deletes the voices beforehand and
        // passes the grace to `delete_with_grace` explicitly); with no
        // sounding gated children this is the legacy immediate teardown.
        let (grace, child_nodes) = {
            let state = self.state.read().await;
            if !state.groups.contains_key(&id) {
                return Err(Error::GroupNotFound(id));
            }
            let mut grace = Duration::ZERO;
            let mut nodes = Vec::new();
            for voice in state.voices.values().filter(|v| v.config.group == id) {
                let sounding = !voice.active_nodes.is_empty() || !voice.note_nodes.is_empty();
                if sounding && voice_is_gated(&voice.config) {
                    grace = grace.max(voice_release_grace(&voice.config));
                    nodes.extend(voice.active_nodes.iter().copied());
                    nodes.extend(voice.note_nodes.values().copied());
                }
            }
            (grace, nodes)
        };
        for node in child_nodes {
            let _ = self.backend.set_param(node, "gate", 0.0).await;
        }
        self.delete_with_grace(id, grace).await
    }

    async fn set_param(&self, id: GroupId, param: &str, value: f32) -> Result<()> {
        let (target_node_id, link_synth_node_id) = {
            let mut state = self.state.write().await;

            let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;

            // Update state
            group.params.insert(param.to_string(), value);

            (group.node_id, group.link_synth_node_id)
        };

        // For amp/pan parameters, target the link synth if it exists
        // This is where audio actually flows through
        let target = if matches!(param, "amp" | "pan") {
            link_synth_node_id.unwrap_or(target_node_id)
        } else {
            target_node_id
        };

        // Send to backend
        self.backend
            .set_param(target, param, value)
            .await
            .map_err(Error::backend)?;

        Ok(())
    }

    async fn mute(&self, id: GroupId, muted: bool) -> Result<()> {
        // De-click: actuate through the link synth's lagged `mute` param
        // (/n_set) instead of pausing the group node (/n_run). The link
        // synth ramps its output over the lag time, and the group's children
        // (including FX tails) keep running while muted, so unmute resumes
        // cleanly instead of releasing a frozen buffer. The `mute` param is
        // separate from `amp`, so the script's amp value is never clobbered.
        let (link, mute_value) = {
            let mut state = self.state.write().await;

            let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;
            group.muted = muted;
            let soloed = group.soloed;
            let link = group.link_synth_node_id;

            let any_soloed = state.groups.values().any(|g| g.soloed);
            let mute_value = if effective_mute(muted, soloed, any_soloed) {
                1.0
            } else {
                0.0
            };
            (link, mute_value)
        };

        // If the link synth isn't spawned yet, the state flag alone is
        // enough — finalize passes it as an initial /s_new arg.
        if let Some(link) = link {
            self.backend
                .set_param(link, "mute", mute_value)
                .await
                .map_err(Error::backend)?;
        }

        Ok(())
    }

    async fn solo(&self, id: GroupId, solo: bool) -> Result<()> {
        // Collect data about what to do, then release lock before backend calls
        let updates: Vec<(NodeId, f32)> = {
            let mut state = self.state.write().await;

            // Update the target group's soloed state
            let group = state.groups.get_mut(&id).ok_or(Error::GroupNotFound(id))?;
            group.soloed = solo;

            // Check if ANY group is now soloed
            let any_soloed = state.groups.values().any(|g| g.soloed);

            // Recompute the effective mute for every group:
            // if any group is soloed, only soloed (and unmuted) groups play;
            // otherwise each group follows its own mute flag. Actuation is
            // the link synth's lagged `mute` param — groups whose link isn't
            // spawned yet are skipped here and picked up at finalize.
            state
                .groups
                .values()
                .filter_map(|g| {
                    g.link_synth_node_id.map(|link| {
                        let mute_value = if effective_mute(g.muted, g.soloed, any_soloed) {
                            1.0
                        } else {
                            0.0
                        };
                        (link, mute_value)
                    })
                })
                .collect()
        };

        // Apply updates to backend (lock released)
        for (link, mute_value) in updates {
            self.backend
                .set_param(link, "mute", mute_value)
                .await
                .map_err(Error::backend)?;
        }

        tracing::debug!("Group {} solo set to {}", id.0, solo);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BufferInfo;
    use crate::compat::Instant;
    use crate::types::{BufferId, EffectId};
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // =========================================================================
    // Mock Backend for Testing
    // =========================================================================

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }

    impl std::error::Error for MockError {}

    struct MockBackend {
        groups_created: AtomicU32,
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        params_set: AtomicU32,
        run_node_calls: AtomicU32,
        synth_names: Mutex<Vec<String>>,
        synth_params: Mutex<Vec<ParamMap>>,
        set_param_calls: Mutex<Vec<(NodeId, String, f32)>>,
        /// `(new group node, target node, add action)` per `create_group`.
        /// Placement is the whole point for a child group — it decides
        /// whether the group runs before or after the nodes that read its
        /// parent's bus — so the mock has to keep it, not just count it.
        group_placements: Mutex<Vec<(NodeId, NodeId, AddAction)>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                groups_created: AtomicU32::new(0),
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                params_set: AtomicU32::new(0),
                run_node_calls: AtomicU32::new(0),
                synth_names: Mutex::new(Vec::new()),
                synth_params: Mutex::new(Vec::new()),
                set_param_calls: Mutex::new(Vec::new()),
                group_placements: Mutex::new(Vec::new()),
            }
        }

        fn groups_created(&self) -> u32 {
            self.groups_created.load(Ordering::Relaxed)
        }

        fn synths_created(&self) -> u32 {
            self.synths_created.load(Ordering::Relaxed)
        }

        fn nodes_freed(&self) -> u32 {
            self.nodes_freed.load(Ordering::Relaxed)
        }

        fn params_set(&self) -> u32 {
            self.params_set.load(Ordering::Relaxed)
        }

        fn run_node_calls(&self) -> u32 {
            self.run_node_calls.load(Ordering::Relaxed)
        }

        fn synth_names(&self) -> Vec<String> {
            self.synth_names.lock().unwrap().clone()
        }

        fn synth_params(&self) -> Vec<ParamMap> {
            self.synth_params.lock().unwrap().clone()
        }

        fn set_param_calls(&self) -> Vec<(NodeId, String, f32)> {
            self.set_param_calls.lock().unwrap().clone()
        }

        fn group_placements(&self) -> Vec<(NodeId, NodeId, AddAction)> {
            self.group_placements.lock().unwrap().clone()
        }

        /// The set_param calls that targeted the `mute` parameter.
        fn mute_calls(&self) -> Vec<(NodeId, f32)> {
            self.set_param_calls()
                .into_iter()
                .filter(|(_, p, _)| p == "mute")
                .map(|(n, _, v)| (n, v))
                .collect()
        }
    }

    #[async_trait::async_trait]
    impl Backend for MockBackend {
        type Error = MockError;

        async fn load_synthdef(
            &self,
            _name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_synth(
            &self,
            def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.synths_created.fetch_add(1, Ordering::Relaxed);
            self.synth_names.lock().unwrap().push(def.to_string());
            self.synth_params.lock().unwrap().push(params.clone());
            Ok(())
        }

        async fn create_group(
            &self,
            node: NodeId,
            target: NodeId,
            action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            self.groups_created.fetch_add(1, Ordering::Relaxed);
            self.group_placements
                .lock()
                .unwrap()
                .push((node, target, action));
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            self.nodes_freed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            self.run_node_calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn set_param(
            &self,
            node: NodeId,
            param: &str,
            value: f32,
        ) -> std::result::Result<(), Self::Error> {
            self.params_set.fetch_add(1, Ordering::Relaxed);
            self.set_param_calls
                .lock()
                .unwrap()
                .push((node, param.to_string(), value));
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 44100,
                channels: 2,
                sample_rate: 44100.0,
            })
        }

        async fn alloc_buffer(
            &self,
            _id: BufferId,
            frames: u32,
            channels: u16,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames,
                channels,
                sample_rate: 44100.0,
            })
        }

        async fn write_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    fn create_handler() -> (
        GroupsHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = GroupsHandler::new(backend.clone(), state.clone());
        (handler, backend, state)
    }

    // =========================================================================
    // Group Creation Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        let result = handler.create(group_id, "TestGroup", None).await;

        assert!(result.is_ok(), "Creating group should succeed");
        assert_eq!(
            backend.groups_created(),
            1,
            "One group should be created in backend"
        );

        let state_read = state.read().await;
        assert!(state_read.groups.contains_key(&group_id));
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(group.name, "TestGroup");
        assert!(group.parent.is_none());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_fails() {
        let (handler, _, _state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.create(group_id, "TestGroup2", None).await;
        assert!(result.is_err(), "Duplicate group creation should fail");
    }

    #[tokio::test]
    async fn test_create_group_with_parent() {
        let (handler, backend, state) = create_handler();

        let parent_id = GroupId::new(1);
        let child_id = GroupId::new(2);

        handler.create(parent_id, "Parent", None).await.unwrap();
        let result = handler.create(child_id, "Child", Some(parent_id)).await;

        assert!(result.is_ok(), "Creating child group should succeed");
        assert_eq!(backend.groups_created(), 2);

        let state_read = state.read().await;
        let child = state_read.groups.get(&child_id).unwrap();
        assert_eq!(child.parent, Some(parent_id));
    }

    /// Cold boot: the parent carries neither fx nor link synth yet, so Tail
    /// on the parent node is right and reproduces the cold-boot tree order.
    #[tokio::test]
    async fn child_group_on_a_bare_parent_lands_at_tail() {
        let (handler, backend, state) = create_handler();
        let parent_id = GroupId::new(1);

        handler.create(parent_id, "Parent", None).await.unwrap();
        handler
            .create(GroupId::new(2), "Child", Some(parent_id))
            .await
            .unwrap();

        let parent_node = state.read().await.groups.get(&parent_id).unwrap().node_id;
        let (_, target, action) = backend.group_placements()[1];
        assert_eq!(
            (target, action),
            (parent_node, AddAction::Tail),
            "with nothing reading the parent bus yet, Tail is the cold-boot order"
        );
    }

    /// A reload that recreates a group hits a parent that already carries its
    /// link synth. Tail-adding there puts the child AFTER the node that reads
    /// the parent's bus: scsynth zeroes audio buses every block, so the whole
    /// child branch goes silent while every synth in it is alive and correct.
    /// This is the board defect — a recall that half-silenced the instrument.
    #[tokio::test]
    async fn child_group_created_after_finalize_precedes_the_parent_link_synth() {
        let (handler, backend, state) = create_handler();
        let parent_id = GroupId::new(1);

        handler.create(parent_id, "Parent", None).await.unwrap();
        // `finalize` is what a first build does — from here on the parent has
        // a link synth sitting at its tail.
        handler.finalize().await.unwrap();

        handler
            .create(GroupId::new(2), "Child", Some(parent_id))
            .await
            .unwrap();

        let link_node = state
            .read()
            .await
            .groups
            .get(&parent_id)
            .unwrap()
            .link_synth_node_id
            .expect("finalize creates the parent's link synth");
        let (_, target, action) = backend.group_placements()[1];
        assert_eq!(
            (target, action),
            (link_node, AddAction::Before),
            "a child group must run before the link synth that reads its parent's bus"
        );
    }

    /// Same hazard one slot earlier: the parent's fx chain reads and rewrites
    /// the parent bus in place, so a child added after the first effect is
    /// unprocessed at best and inaudible at worst.
    #[tokio::test]
    async fn child_group_precedes_the_parents_first_effect() {
        let (handler, backend, state) = create_handler();
        let parent_id = GroupId::new(1);

        handler.create(parent_id, "Parent", None).await.unwrap();
        handler.finalize().await.unwrap();

        // An effect on the parent, placed as `EffectsHandler::add` places it:
        // between the voices and the link synth.
        let fx_node = NodeId::new(4242);
        {
            let mut state = state.write().await;
            let audio_bus = state.groups.get(&parent_id).unwrap().audio_bus;
            state.effects.insert(
                EffectId::new(7),
                crate::state::EffectState {
                    id: EffectId::new(7),
                    group: parent_id,
                    synthdef: "ladder_filter".to_string(),
                    node_id: fx_node,
                    audio_bus,
                    params: ParamMap::new(),
                },
            );
            state
                .group_effect_chain
                .insert(parent_id, vec![EffectId::new(7)]);
        }

        handler
            .create(GroupId::new(2), "Child", Some(parent_id))
            .await
            .unwrap();

        let (_, target, action) = backend.group_placements()[1];
        assert_eq!(
            (target, action),
            (fx_node, AddAction::Before),
            "a child group must run before the fx chain that rewrites its parent's bus"
        );
    }

    #[tokio::test]
    async fn test_create_group_with_nonexistent_parent_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler
            .create(GroupId::new(1), "Child", Some(GroupId::new(999)))
            .await;
        assert!(result.is_err(), "Should fail with non-existent parent");
    }

    #[tokio::test]
    async fn test_create_group_allocates_node_and_bus() {
        let (handler, _, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();

        // Node ID should be > 0 (0 is reserved for root)
        assert!(group.node_id.0 > 0, "Node ID should be allocated");
        // Audio bus should be in the group bus range (>= 16)
        assert!(
            group.audio_bus.0 >= 16,
            "Audio bus should be in group range"
        );
    }

    // =========================================================================
    // Group Deletion Tests
    // =========================================================================

    #[tokio::test]
    async fn test_delete_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.delete(group_id).await;
        assert!(result.is_ok(), "Deleting group should succeed");
        assert_eq!(backend.nodes_freed(), 1, "Node should be freed");

        let state_read = state.read().await;
        assert!(!state_read.groups.contains_key(&group_id));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.delete(GroupId::new(999)).await;
        assert!(result.is_err(), "Deleting non-existent group should fail");
    }

    // =========================================================================
    // Group Set Param Tests
    // =========================================================================

    #[tokio::test]
    async fn test_set_param() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.set_param(group_id, "gain", 0.8).await;
        assert!(result.is_ok(), "Setting param should succeed");
        assert_eq!(backend.params_set(), 1);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(*group.params.get("gain").unwrap(), 0.8);
    }

    #[tokio::test]
    async fn test_set_param_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.set_param(GroupId::new(999), "gain", 0.5).await;
        assert!(
            result.is_err(),
            "Setting param on non-existent group should fail"
        );
    }

    // =========================================================================
    // Group Mute Tests
    //
    // Mute/solo actuate through the link synth's lagged `mute` param via
    // /n_set — never through /n_run (which pauses children mid-sample and
    // freezes FX tails).
    // =========================================================================

    /// Fetch a group's link synth node id (panics if not finalized).
    async fn link_node(state: &Arc<RwLock<State>>, id: GroupId) -> NodeId {
        state
            .read()
            .await
            .groups
            .get(&id)
            .unwrap()
            .link_synth_node_id
            .expect("group not finalized")
    }

    #[tokio::test]
    async fn test_mute_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.finalize().await.unwrap();
        let link = link_node(&state, group_id).await;

        let result = handler.mute(group_id, true).await;
        assert!(result.is_ok(), "Muting group should succeed");
        assert_eq!(
            backend.run_node_calls(),
            0,
            "mute must not pause the group node via /n_run"
        );
        assert_eq!(
            backend.mute_calls(),
            vec![(link, 1.0)],
            "mute must set the link synth's mute param to 1.0"
        );

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(group.muted);
    }

    #[tokio::test]
    async fn test_unmute_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.finalize().await.unwrap();
        let link = link_node(&state, group_id).await;

        handler.mute(group_id, true).await.unwrap();
        handler.mute(group_id, false).await.unwrap();

        assert_eq!(
            backend.mute_calls(),
            vec![(link, 1.0), (link, 0.0)],
            "unmute must set the link synth's mute param back to 0.0"
        );

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(!group.muted);
    }

    #[tokio::test]
    async fn test_mute_before_finalize_applies_at_finalize() {
        // Muting a group whose link synth isn't spawned yet must not fail;
        // the mute state is passed as an initial /s_new arg at finalize.
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        handler.mute(group_id, true).await.unwrap();
        assert!(
            backend.mute_calls().is_empty(),
            "no link synth yet — nothing to actuate"
        );

        handler.finalize().await.unwrap();
        let params = backend.synth_params();
        assert_eq!(params.len(), 1);
        assert_eq!(
            params[0].get("mute"),
            Some(&1.0),
            "finalize must spawn the link synth pre-muted"
        );

        let state_read = state.read().await;
        assert!(state_read.groups.get(&group_id).unwrap().muted);
    }

    #[tokio::test]
    async fn test_mute_does_not_clobber_group_amp() {
        // The mute gate is a separate synth param — the script's amp value
        // must survive a mute/unmute cycle untouched.
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.set_param(group_id, "amp", 0.7).await.unwrap();
        handler.finalize().await.unwrap();

        let amp_sets_before = backend
            .set_param_calls()
            .iter()
            .filter(|(_, p, _)| p == "amp")
            .count();

        handler.mute(group_id, true).await.unwrap();
        handler.mute(group_id, false).await.unwrap();

        let amp_sets_after = backend
            .set_param_calls()
            .iter()
            .filter(|(_, p, _)| p == "amp")
            .count();
        assert_eq!(
            amp_sets_before, amp_sets_after,
            "mute/unmute must not touch the amp param"
        );

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(*group.params.get("amp").unwrap(), 0.7);
    }

    #[tokio::test]
    async fn test_mute_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.mute(GroupId::new(999), true).await;
        assert!(result.is_err());
    }

    // =========================================================================
    // Group Solo Tests
    // =========================================================================

    #[tokio::test]
    async fn test_solo_group() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.finalize().await.unwrap();
        let link = link_node(&state, group_id).await;

        let result = handler.solo(group_id, true).await;
        assert!(result.is_ok(), "Soloing group should succeed");
        assert_eq!(backend.run_node_calls(), 0, "solo must not use /n_run");
        assert_eq!(
            backend.mute_calls(),
            vec![(link, 0.0)],
            "the soloed group itself stays audible"
        );

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(group.soloed);
    }

    #[tokio::test]
    async fn test_unsolo_group() {
        let (handler, _, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.finalize().await.unwrap();

        handler.solo(group_id, true).await.unwrap();
        handler.solo(group_id, false).await.unwrap();

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(!group.soloed);
    }

    #[tokio::test]
    async fn test_solo_nonexistent_group_fails() {
        let (handler, _, _state) = create_handler();

        let result = handler.solo(GroupId::new(999), true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_solo_affects_multiple_groups() {
        let (handler, backend, state) = create_handler();

        let group1 = GroupId::new(1);
        let group2 = GroupId::new(2);

        handler.create(group1, "Group1", None).await.unwrap();
        handler.create(group2, "Group2", None).await.unwrap();
        handler.finalize().await.unwrap();
        let link1 = link_node(&state, group1).await;
        let link2 = link_node(&state, group2).await;

        // Solo group1: group1 stays audible, group2 gets muted.
        handler.solo(group1, true).await.unwrap();

        let mut calls = backend.mute_calls();
        calls.sort_by_key(|(n, _)| n.0);
        let mut expected = vec![(link1, 0.0), (link2, 1.0)];
        expected.sort_by_key(|(n, _)| n.0);
        assert_eq!(
            calls, expected,
            "solo must mute all other groups via their link synths"
        );
        assert_eq!(backend.run_node_calls(), 0, "solo must not use /n_run");

        // Unsolo: both groups become audible again.
        handler.solo(group1, false).await.unwrap();
        let calls = backend.mute_calls();
        assert_eq!(calls.len(), 4);
        assert!(
            calls[2..].iter().all(|(_, v)| *v == 0.0),
            "unsolo must unmute all groups: {:?}",
            calls
        );
    }

    #[tokio::test]
    async fn test_solo_respects_muted_group() {
        // A muted group must stay muted even when soloed.
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.finalize().await.unwrap();
        let link = link_node(&state, group_id).await;

        handler.mute(group_id, true).await.unwrap();
        handler.solo(group_id, true).await.unwrap();

        assert_eq!(
            backend.mute_calls(),
            vec![(link, 1.0), (link, 1.0)],
            "soloing a muted group must keep it muted"
        );
    }

    #[tokio::test]
    async fn test_finalize_applies_solo_state() {
        // Solo before finalize: the non-soloed group's link synth must spawn
        // pre-muted.
        let (handler, backend, _state) = create_handler();

        let group1 = GroupId::new(1);
        let group2 = GroupId::new(2);
        handler.create(group1, "Soloed", None).await.unwrap();
        handler.create(group2, "Other", None).await.unwrap();

        handler.solo(group1, true).await.unwrap();
        handler.finalize().await.unwrap();

        let params = backend.synth_params();
        assert_eq!(params.len(), 2);
        let mutes: Vec<f32> = params.iter().map(|p| *p.get("mute").unwrap()).collect();
        assert!(
            mutes.contains(&0.0) && mutes.contains(&1.0),
            "soloed group spawns unmuted, the other pre-muted: {:?}",
            mutes
        );
    }

    // =========================================================================
    // Finalize Tests
    // =========================================================================

    #[tokio::test]
    async fn test_finalize_creates_link_synths() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        let result = handler.finalize().await;
        assert!(result.is_ok(), "Finalize should succeed");
        assert_eq!(backend.synths_created(), 1, "Link synth should be created");

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert!(
            group.link_synth_node_id.is_some(),
            "Link synth node ID should be set"
        );
    }

    #[tokio::test]
    async fn test_finalize_initializes_link_synth_amp_from_group_param() {
        let (handler, backend, _state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.set_param(group_id, "amp", 0.25).await.unwrap();

        handler.finalize().await.unwrap();

        let params = backend.synth_params();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].get("amp"), Some(&0.25));
    }

    #[tokio::test]
    async fn test_finalize_multiple_groups() {
        let (handler, backend, _state) = create_handler();

        let group1 = GroupId::new(1);
        let group2 = GroupId::new(2);

        handler.create(group1, "Group1", None).await.unwrap();
        handler.create(group2, "Group2", None).await.unwrap();

        handler.finalize().await.unwrap();

        assert_eq!(backend.synths_created(), 2, "Link synth for each group");
    }

    #[tokio::test]
    async fn test_finalize_default_spawns_stereo_link_synth() {
        // Default group (output_channels = None, output_bus = None) keeps the
        // legacy stereo behaviour: spawn `system_link_audio`.
        let (handler, backend, _state) = create_handler();
        handler
            .create(GroupId::new(1), "stereo", None)
            .await
            .unwrap();
        handler.finalize().await.unwrap();
        assert_eq!(backend.synth_names(), vec!["system_link_audio".to_string()]);
    }

    #[tokio::test]
    async fn test_finalize_mono_group_spawns_mono_link_synth() {
        // Mono-routed group (output_channels = Some(1), output_bus = Some(2))
        // dispatches to the mixdown variant.
        let (handler, backend, state) = create_handler();
        let group_id = GroupId::new(1);
        handler.create(group_id, "mono", None).await.unwrap();

        // Mirror what the Rhai layer (Task C) will do once it sets both
        // fields together: mono output to hardware bus 2.
        {
            let mut state = state.write().await;
            let group = state.groups.get_mut(&group_id).unwrap();
            group.output_bus = Some(2);
            group.output_channels = Some(1);
        }

        handler.finalize().await.unwrap();
        assert_eq!(
            backend.synth_names(),
            vec!["system_link_audio_mono".to_string()],
            "output_channels=Some(1) must spawn the mono mixdown variant"
        );
    }

    #[tokio::test]
    async fn test_finalize_stereo_group_with_output_bus_spawns_stereo() {
        // Stereo-routed group (output_channels = Some(2), output_bus = Some(2))
        // keeps the existing `system_link_audio` synthdef.
        let (handler, backend, state) = create_handler();
        let group_id = GroupId::new(1);
        handler.create(group_id, "stereo_hw", None).await.unwrap();
        {
            let mut state = state.write().await;
            let group = state.groups.get_mut(&group_id).unwrap();
            group.output_bus = Some(2);
            group.output_channels = Some(2);
        }

        handler.finalize().await.unwrap();
        assert_eq!(backend.synth_names(), vec!["system_link_audio".to_string()]);
    }

    #[tokio::test]
    async fn test_finalize_idempotent() {
        let (handler, backend, _state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        // Finalize twice
        handler.finalize().await.unwrap();
        handler.finalize().await.unwrap();

        // Should only create one link synth (the second finalize is a no-op)
        assert_eq!(
            backend.synths_created(),
            1,
            "Should only create link synth once"
        );
    }

    // =========================================================================
    // Multiple Operations Tests
    // =========================================================================

    #[tokio::test]
    async fn test_create_delete_create() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);

        handler.create(group_id, "TestGroup", None).await.unwrap();
        handler.delete(group_id).await.unwrap();
        handler.create(group_id, "TestGroup2", None).await.unwrap();

        assert_eq!(backend.groups_created(), 2);
        assert_eq!(backend.nodes_freed(), 1);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(group.name, "TestGroup2");
    }

    #[tokio::test]
    async fn test_multiple_params() {
        let (handler, backend, state) = create_handler();

        let group_id = GroupId::new(1);
        handler.create(group_id, "TestGroup", None).await.unwrap();

        handler.set_param(group_id, "gain", 0.8).await.unwrap();
        handler.set_param(group_id, "pan", -0.5).await.unwrap();
        handler.set_param(group_id, "filter", 0.6).await.unwrap();

        assert_eq!(backend.params_set(), 3);

        let state_read = state.read().await;
        let group = state_read.groups.get(&group_id).unwrap();
        assert_eq!(*group.params.get("gain").unwrap(), 0.8);
        assert_eq!(*group.params.get("pan").unwrap(), -0.5);
        assert_eq!(*group.params.get("filter").unwrap(), 0.6);
    }
}
