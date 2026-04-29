//! Per-voice output routing.
//!
//! Each voice declares one or more output ports (named audio buses, allocated
//! by Story 2's [`VoiceState::output_buses`](crate::state::VoiceState::output_buses)).
//! A `RouteDest` says where a single port's signal should go: into a group's
//! mix bus, straight to main, or discarded.
//!
//! Story 6a wired up the type registry and the diff machinery only. Story 6b
//! (this file) realizes the diff: [`RoutesHandler::finalize`] instantiates a
//! `port_to_group_link_<channels>` mixer synth for each added route, frees
//! the mixer for each removed route, and swaps the mixer for changed routes.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::State;
use crate::types::{BusId, GroupId, NodeId, ParamMap, VoiceId};
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use vibelang_dsp::OutputPort;

/// Where a voice's output port should send its audio.
///
/// - `Group(id)` mixes into the named group's audio bus.
/// - `Main` sends directly to bus 0 (the hardware main output), bypassing groups.
/// - `Muted` discards the signal.
///
/// CV-to-param routing is deferred to a later story.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RouteDest {
    Group(GroupId),
    Main,
    Muted,
}

/// A single concrete route: one voice's named port → one destination.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Route {
    pub voice_id: VoiceId,
    pub port_name: String,
    pub dest: RouteDest,
}

/// Per-voice route registry, keyed by `(voice_id, port_name)`.
///
/// Stored on [`ScriptState`](crate::reload::ScriptState) so script-side mutations
/// (Rhai surface lands in Story 8) can carry the desired route map across reloads.
pub type RouteMap = HashMap<(VoiceId, String), RouteDest>;

/// Compute the count-based default route entries for a freshly created voice.
///
/// Story 5: walks the synthdef's declared output ports and produces a default
/// destination for each, applying these rules by *port count*:
///
/// - **0 ports** (degenerate): no defaults.
/// - **1 port** (mono or stereo): the port routes into the voice's own group.
///   The mixer-synth selection (`port_to_group_link_1` for mono / `_2` for
///   stereo) is decided later by [`RoutesHandler::spawn_route`] based on the
///   port's channel count, so the helper just emits a `Group(voice_group)`
///   destination — mono will dup-pan to L/R, stereo passes straight through.
/// - **2 ports** (typically mono+mono): both ports route into the voice's
///   group; their dup-panned signals sum at the group bus, giving a
///   "dual-mono summed" image that's effectively mono-centered.
/// - **N > 2 ports**: only the first two ports get defaults; the remaining
///   ports stay un-routed (no entry returned), which the runtime treats as
///   silent until the user installs an explicit route.
///
/// The helper ignores any user-supplied routes — it just returns the *suggested*
/// defaults. Callers are responsible for storing them somewhere that the route
/// merge step can consult (see [`State::default_routes`](crate::state::State)),
/// and for letting explicit user routes override the defaults at merge time.
pub fn default_routes_for_voice(
    voice_group: GroupId,
    ports: &[OutputPort],
) -> Vec<(String, RouteDest)> {
    let count = match ports.len() {
        0 => 0,
        1 | 2 => ports.len(),
        _ => 2,
    };
    ports
        .iter()
        .take(count)
        .map(|p| (p.name.clone(), RouteDest::Group(voice_group)))
        .collect()
}

/// Merge default routes with explicit (script-supplied) user routes.
///
/// Story 5 reconciliation rule: an explicit `(voice_id, port_name)` entry in
/// `user` always wins over the matching default in `defaults`. Defaults are
/// only used for keys that have no explicit entry. The returned map is what
/// the runtime should diff against `current_routes` to decide which mixer
/// synths to spawn or free.
pub fn merge_default_routes(user: &RouteMap, defaults: &RouteMap) -> RouteMap {
    let mut merged = defaults.clone();
    for (key, dest) in user {
        merged.insert(key.clone(), dest.clone());
    }
    merged
}

/// Description of how an existing route's destination changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteChange {
    pub voice_id: VoiceId,
    pub port_name: String,
    pub old_dest: RouteDest,
    pub new_dest: RouteDest,
}

/// Difference between two route maps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteDiff {
    /// Keys present in `new` but not in `old`.
    pub additions: Vec<Route>,
    /// Keys present in `old` but not in `new`. Carries the prior `dest`.
    pub removals: Vec<Route>,
    /// Keys present in both, with a different `dest`.
    pub changes: Vec<RouteChange>,
}

impl RouteDiff {
    /// True when there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty() && self.changes.is_empty()
    }
}

/// Computes and applies per-voice routing changes.
///
/// The handler holds the backend and shared state; [`Self::diff`] is a pure
/// static helper (no `self`) and [`Self::finalize`] is the imperative method
/// that emits backend calls.
pub struct RoutesHandler<B: Backend> {
    backend: Arc<B>,
    state: Arc<RwLock<State>>,
}

impl<B: Backend> RoutesHandler<B> {
    /// Create a new routes handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Compute the additions, removals, and destination changes between two route maps.
    pub fn diff(old: &RouteMap, new: &RouteMap) -> RouteDiff {
        let mut diff = RouteDiff::default();

        for ((voice_id, port_name), new_dest) in new {
            match old.get(&(*voice_id, port_name.clone())) {
                None => diff.additions.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: new_dest.clone(),
                }),
                Some(old_dest) if old_dest != new_dest => diff.changes.push(RouteChange {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    old_dest: old_dest.clone(),
                    new_dest: new_dest.clone(),
                }),
                _ => {}
            }
        }

        for ((voice_id, port_name), old_dest) in old {
            if !new.contains_key(&(*voice_id, port_name.clone())) {
                diff.removals.push(Route {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    dest: old_dest.clone(),
                });
            }
        }

        diff
    }

    /// Apply a route diff: free old mixer synths, instantiate new ones.
    ///
    /// Order:
    /// 1. Free mixer synths for *removals* and the old destination of *changes*.
    /// 2. Instantiate mixer synths for *additions* and the new destination of *changes*.
    ///
    /// Step 1 runs before step 2 so a moved route can reuse the same node-id
    /// pool slot; both steps drop the state lock before each backend call to
    /// preserve the project's lock discipline.
    pub async fn finalize(&self, diff: &RouteDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }

        // ============================================================
        // Step 1: free old mixers (removals + old side of changes)
        // ============================================================
        let nodes_to_free: Vec<NodeId> = {
            let mut state = self.state.write().await;
            let mut nodes = Vec::new();
            for r in &diff.removals {
                if let Some(node_id) = state
                    .route_synths
                    .remove(&(r.voice_id, r.port_name.clone()))
                {
                    state.free_node_id(node_id);
                    nodes.push(node_id);
                } else {
                    tracing::debug!(
                        "RoutesHandler::finalize: no live mixer for removed route voice={:?} port={:?} (already freed?)",
                        r.voice_id,
                        r.port_name
                    );
                }
            }
            for c in &diff.changes {
                if let Some(node_id) = state
                    .route_synths
                    .remove(&(c.voice_id, c.port_name.clone()))
                {
                    state.free_node_id(node_id);
                    nodes.push(node_id);
                }
            }
            nodes
        };
        for node_id in nodes_to_free {
            tracing::debug!("RoutesHandler::finalize: freeing mixer node {:?}", node_id);
            let _ = self.backend.free_node(node_id).await;
        }

        // ============================================================
        // Step 2: spawn new mixers (additions + new side of changes)
        // ============================================================
        for r in &diff.additions {
            if let Err(e) = self.spawn_route(r.voice_id, &r.port_name, &r.dest).await {
                tracing::warn!(
                    "RoutesHandler::finalize: failed to spawn mixer for addition {:?}/{:?}: {}",
                    r.voice_id,
                    r.port_name,
                    e
                );
            }
        }
        for c in &diff.changes {
            if let Err(e) = self
                .spawn_route(c.voice_id, &c.port_name, &c.new_dest)
                .await
            {
                tracing::warn!(
                    "RoutesHandler::finalize: failed to spawn mixer for change {:?}/{:?}: {}",
                    c.voice_id,
                    c.port_name,
                    e
                );
            }
        }

        Ok(())
    }

    /// Instantiate one mixer synth for `(voice, port) → dest`.
    ///
    /// `Muted` destinations short-circuit (no mixer is created). `Main` routes
    /// target bus 0 (hardware stereo out); `Group(id)` routes target the group's
    /// audio bus. The synthdef variant (`port_to_group_link_1` vs `_2`) is
    /// chosen from the port's declared channel count.
    async fn spawn_route(
        &self,
        voice_id: VoiceId,
        port_name: &str,
        dest: &RouteDest,
    ) -> Result<()> {
        if matches!(dest, RouteDest::Muted) {
            tracing::debug!(
                "RoutesHandler: muted route voice={:?} port={:?} — skipping mixer",
                voice_id,
                port_name
            );
            return Ok(());
        }

        let (node_id, group_node, in_bus, channels, out_bus) = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get(&voice_id)
                .ok_or(Error::VoiceNotFound(voice_id))?;

            let in_bus = voice
                .output_buses
                .iter()
                .find(|(name, _)| name == port_name)
                .map(|(_, bus)| *bus)
                .ok_or_else(|| {
                    Error::SynthDefNotFound(format!(
                        "port {:?} on voice {:?}",
                        port_name, voice_id
                    ))
                })?;

            let synthdef = voice.config.synthdef.clone();
            let voice_group_id = voice.config.group;

            let ports = state.synthdef_outputs(&synthdef);
            let channels = ports
                .iter()
                .find(|p| p.name == port_name)
                .map(|p| p.channels)
                .unwrap_or(2);

            // The mixer synth is parented on the *voice's* group node so that
            // it sits in tree order between voice synths (added at Tail) and
            // the group's link synth (effects insert Before link). This gives
            // the runtime tick order: voices → routes → effects → link.
            let voice_group = state
                .groups
                .get(&voice_group_id)
                .ok_or(Error::GroupNotFound(voice_group_id))?;
            let group_node = voice_group.node_id;

            let out_bus = match dest {
                RouteDest::Group(g) => {
                    let dest_group = state.groups.get(g).ok_or(Error::GroupNotFound(*g))?;
                    dest_group.audio_bus
                }
                RouteDest::Main => BusId::new(0),
                RouteDest::Muted => unreachable!("filtered above"),
            };

            let node_id = state.alloc_node_id();
            state
                .route_synths
                .insert((voice_id, port_name.to_string()), node_id);

            (node_id, group_node, in_bus, channels, out_bus)
        };

        let synthdef_name = match channels {
            1 => "port_to_group_link_1",
            2 => "port_to_group_link_2",
            n => {
                return Err(Error::SynthDefNotFound(format!(
                    "no port_to_group_link_<channels> built-in for {} channels",
                    n
                )));
            }
        };

        let mut params = ParamMap::new();
        params.insert("in_bus".to_string(), in_bus.0 as f32);
        params.insert("out_bus".to_string(), out_bus.0 as f32);

        tracing::debug!(
            "RoutesHandler: spawning {} (node={:?}) voice={:?} port={:?} in_bus={} out_bus={} group_node={:?}",
            synthdef_name,
            node_id,
            voice_id,
            port_name,
            in_bus.0,
            out_bus.0,
            group_node
        );

        self.backend
            .create_synth(
                synthdef_name,
                node_id,
                group_node,
                AddAction::Tail,
                &params,
            )
            .await
            .map_err(Error::backend)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::BufferInfo;
    use crate::compat::Instant;
    use crate::state::{GroupState, VoiceState};
    use crate::traits::VoiceConfig;
    use crate::types::{BufferId, ParamMap};
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // =========================================================================
    // Default-routes helper tests (Story 5)
    // =========================================================================

    fn port(name: &str, channels: u8) -> OutputPort {
        OutputPort {
            name: name.to_string(),
            channels,
            rate: vibelang_dsp::PortRate::Ar,
        }
    }

    #[test]
    fn default_routes_one_mono_port_routes_into_voice_group() {
        // 1 mono port → pan-mono into group L/R. The pan-mono behaviour is
        // realised by `port_to_group_link_1` (mono→stereo dup); the helper
        // just emits the destination.
        let group = GroupId::new(7);
        let routes = default_routes_for_voice(group, &[port("out", 1)]);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].0, "out");
        assert_eq!(routes[0].1, RouteDest::Group(group));
    }

    #[test]
    fn default_routes_one_stereo_port_routes_straight_into_group() {
        // 1 stereo port → straight L/R into group (today's legacy behaviour).
        let group = GroupId::new(11);
        let routes = default_routes_for_voice(group, &[port("out", 2)]);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].0, "out");
        assert_eq!(routes[0].1, RouteDest::Group(group));
    }

    #[test]
    fn default_routes_two_mono_ports_both_route_into_group() {
        // 2 mono ports → port[0]=L, port[1]=R into the group bus, summed at
        // the destination (dual-mono summed). Helper emits one route per port.
        let group = GroupId::new(3);
        let routes = default_routes_for_voice(group, &[port("L", 1), port("R", 1)]);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].0, "L");
        assert_eq!(routes[0].1, RouteDest::Group(group));
        assert_eq!(routes[1].0, "R");
        assert_eq!(routes[1].1, RouteDest::Group(group));
    }

    #[test]
    fn default_routes_four_ports_only_first_two_default_routed() {
        // N>2 ports (e.g. spectraphon side: sine, sub, odd, even): only the
        // first two get defaults; the rest stay un-routed (silent).
        let group = GroupId::new(2);
        let ports = [
            port("sine", 1),
            port("sub", 1),
            port("odd", 2),
            port("even", 2),
        ];
        let routes = default_routes_for_voice(group, &ports);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].0, "sine");
        assert_eq!(routes[1].0, "sub");
        // No entries for "odd" or "even".
        assert!(routes.iter().all(|(n, _)| n != "odd" && n != "even"));
    }

    #[test]
    fn default_routes_zero_ports_returns_empty() {
        let group = GroupId::new(1);
        let routes = default_routes_for_voice(group, &[]);
        assert!(routes.is_empty());
    }

    #[test]
    fn merge_default_routes_user_overrides_default() {
        // Explicit user route on a port wins over the default — the merge
        // step is what enforces "explicit user route on port[0] not
        // overridden by default."
        let voice_id = VoiceId::new(42);
        let group_default = GroupId::new(1);
        let group_user = GroupId::new(99);

        let mut defaults = RouteMap::new();
        defaults.insert(
            (voice_id, "out".to_string()),
            RouteDest::Group(group_default),
        );

        let mut user = RouteMap::new();
        user.insert((voice_id, "out".to_string()), RouteDest::Group(group_user));

        let merged = merge_default_routes(&user, &defaults);

        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[&(voice_id, "out".to_string())],
            RouteDest::Group(group_user),
            "user route must win over default"
        );
    }

    #[test]
    fn merge_default_routes_default_fills_unrouted_port() {
        let voice_id = VoiceId::new(5);
        let group = GroupId::new(8);

        let mut defaults = RouteMap::new();
        defaults.insert((voice_id, "L".to_string()), RouteDest::Group(group));
        defaults.insert((voice_id, "R".to_string()), RouteDest::Group(group));

        let mut user = RouteMap::new();
        user.insert((voice_id, "L".to_string()), RouteDest::Main);

        let merged = merge_default_routes(&user, &defaults);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[&(voice_id, "L".to_string())], RouteDest::Main);
        assert_eq!(
            merged[&(voice_id, "R".to_string())],
            RouteDest::Group(group),
            "port without explicit route falls back to default"
        );
    }

    #[test]
    fn merge_default_routes_empty_user_returns_defaults() {
        let voice_id = VoiceId::new(1);
        let group = GroupId::new(1);
        let mut defaults = RouteMap::new();
        defaults.insert((voice_id, "out".to_string()), RouteDest::Group(group));

        let merged = merge_default_routes(&RouteMap::new(), &defaults);
        assert_eq!(merged, defaults);
    }

    // =========================================================================
    // Diff tests (Story 6a — preserved)
    // =========================================================================

    fn make_map(entries: &[((u32, &str), RouteDest)]) -> RouteMap {
        entries
            .iter()
            .map(|((vid, port), dest)| ((VoiceId::new(*vid), (*port).to_string()), dest.clone()))
            .collect()
    }

    #[test]
    fn diff_empty_old_to_n_entries_returns_n_additions() {
        let old = RouteMap::new();
        let new = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "left"), RouteDest::Main),
            ((2, "right"), RouteDest::Muted),
        ]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert_eq!(diff.additions.len(), 3);
        assert!(diff.removals.is_empty());
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_identical_returns_empty() {
        let routes = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "left"), RouteDest::Main),
            ((2, "right"), RouteDest::Muted),
        ]);

        let diff = RoutesHandler::<MockBackend>::diff(&routes, &routes);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_addition_returns_one_addition_only() {
        let old = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);
        let new = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "out"), RouteDest::Main),
        ]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert_eq!(diff.additions.len(), 1);
        let added = &diff.additions[0];
        assert_eq!(added.voice_id, VoiceId::new(2));
        assert_eq!(added.port_name, "out");
        assert_eq!(added.dest, RouteDest::Main);
        assert!(diff.removals.is_empty());
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_removal_returns_one_removal_only() {
        let old = make_map(&[
            ((1, "out"), RouteDest::Group(GroupId::new(1))),
            ((2, "out"), RouteDest::Main),
        ]);
        let new = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert_eq!(diff.removals.len(), 1);
        let removed = &diff.removals[0];
        assert_eq!(removed.voice_id, VoiceId::new(2));
        assert_eq!(removed.port_name, "out");
        assert_eq!(removed.dest, RouteDest::Main);
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn diff_changed_dest_returns_one_change_only() {
        let old = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);
        let new = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(2)))]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert!(diff.removals.is_empty());
        assert_eq!(diff.changes.len(), 1);
        let chg = &diff.changes[0];
        assert_eq!(chg.voice_id, VoiceId::new(1));
        assert_eq!(chg.port_name, "out");
        assert_eq!(chg.old_dest, RouteDest::Group(GroupId::new(1)));
        assert_eq!(chg.new_dest, RouteDest::Group(GroupId::new(2)));
    }

    #[test]
    fn diff_changed_dest_main_to_muted() {
        let old = make_map(&[((3, "fx_send"), RouteDest::Main)]);
        let new = make_map(&[((3, "fx_send"), RouteDest::Muted)]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert_eq!(diff.changes.len(), 1);
        assert_eq!(diff.changes[0].old_dest, RouteDest::Main);
        assert_eq!(diff.changes[0].new_dest, RouteDest::Muted);
    }

    // =========================================================================
    // Mock backend
    // =========================================================================

    #[derive(Debug)]
    struct MockError;

    impl std::fmt::Display for MockError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock error")
        }
    }
    impl std::error::Error for MockError {}

    #[derive(Debug, Clone)]
    struct CreateSynthCall {
        def: String,
        node: NodeId,
        #[allow(dead_code)]
        target: NodeId,
        in_bus: f32,
        out_bus: f32,
    }

    struct MockBackend {
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        last_creates: Mutex<Vec<CreateSynthCall>>,
        last_frees: Mutex<Vec<NodeId>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                last_creates: Mutex::new(Vec::new()),
                last_frees: Mutex::new(Vec::new()),
            }
        }
        fn synths_created(&self) -> u32 {
            self.synths_created.load(Ordering::Relaxed)
        }
        fn nodes_freed(&self) -> u32 {
            self.nodes_freed.load(Ordering::Relaxed)
        }
        fn creates(&self) -> Vec<CreateSynthCall> {
            self.last_creates.lock().unwrap().clone()
        }
        fn frees(&self) -> Vec<NodeId> {
            self.last_frees.lock().unwrap().clone()
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
            node: NodeId,
            target: NodeId,
            _action: AddAction,
            params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            self.synths_created.fetch_add(1, Ordering::Relaxed);
            self.last_creates.lock().unwrap().push(CreateSynthCall {
                def: def.to_string(),
                node,
                target,
                in_bus: *params.get("in_bus").unwrap_or(&-1.0),
                out_bus: *params.get("out_bus").unwrap_or(&-1.0),
            });
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_node(&self, node: NodeId) -> std::result::Result<(), Self::Error> {
            self.nodes_freed.fetch_add(1, Ordering::Relaxed);
            self.last_frees.lock().unwrap().push(node);
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(
            &self,
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 0,
                channels: 0,
                sample_rate: 0.0,
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
                sample_rate: 0.0,
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
    // Finalize harness
    // =========================================================================

    /// Build a state pre-populated with a voice routed to group `dest_group`.
    /// Returns (handler, backend, state, voice_id, port_name, dest_group_id).
    async fn setup_voice_in_group(
        port_channels: u8,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        VoiceId,
        String,
        GroupId,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let voice_id = VoiceId::new(42);
        let voice_group_id = GroupId::new(1);
        let dest_group_id = GroupId::new(2);
        let port_name = "out".to_string();

        {
            let mut s = state.write().await;
            s.synthdefs.insert("test_synth".to_string());
            s.synthdef_outputs.insert(
                "test_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: port_name.clone(),
                    channels: port_channels,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );

            let voice_group_node = s.alloc_node_id();
            let voice_group_bus = s.alloc_audio_bus(2);
            s.groups.insert(
                voice_group_id,
                GroupState {
                    id: voice_group_id,
                    name: "voice_group".to_string(),
                    parent: None,
                    node_id: voice_group_node,
                    audio_bus: voice_group_bus,
                    link_synth_node_id: None,
                    muted: false,
                    soloed: false,
                    params: ParamMap::new(),
                    output_bus: None,
                },
            );

            let dest_node = s.alloc_node_id();
            let dest_bus = s.alloc_audio_bus(2);
            s.groups.insert(
                dest_group_id,
                GroupState {
                    id: dest_group_id,
                    name: "dest".to_string(),
                    parent: None,
                    node_id: dest_node,
                    audio_bus: dest_bus,
                    link_synth_node_id: None,
                    muted: false,
                    soloed: false,
                    params: ParamMap::new(),
                    output_bus: None,
                },
            );

            let port_bus = s.alloc_audio_bus(port_channels);
            s.voices.insert(
                voice_id,
                VoiceState {
                    id: voice_id,
                    config: VoiceConfig::new("v", "test_synth", voice_group_id),
                    active_nodes: Vec::new(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![(port_name.clone(), port_bus)],
                },
            );
        }

        (
            handler,
            backend,
            state,
            voice_id,
            port_name,
            dest_group_id,
        )
    }

    #[tokio::test]
    async fn finalize_addition_creates_one_mixer_synth_for_default_group_route() {
        // One voice, one default-routed port: one mixer synth created, audible
        // signal at group bus.
        let (handler, backend, state, voice_id, port_name, dest_group_id) =
            setup_voice_in_group(2).await;

        let mut diff = RouteDiff::default();
        diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_group_id),
        });

        handler.finalize(&diff).await.unwrap();

        assert_eq!(backend.synths_created(), 1, "one mixer synth created");
        assert_eq!(backend.nodes_freed(), 0, "nothing freed on pure addition");

        let creates = backend.creates();
        assert_eq!(creates[0].def, "port_to_group_link_2");

        let dest_bus = state
            .read()
            .await
            .groups
            .get(&dest_group_id)
            .unwrap()
            .audio_bus
            .0 as f32;
        assert_eq!(creates[0].out_bus, dest_bus);

        let port_bus = state
            .read()
            .await
            .voices
            .get(&voice_id)
            .unwrap()
            .output_buses[0]
            .1
             .0 as f32;
        assert_eq!(creates[0].in_bus, port_bus);

        // route_synths must remember the node so a later removal can free it.
        assert!(state
            .read()
            .await
            .route_synths
            .contains_key(&(voice_id, port_name)));
    }

    #[tokio::test]
    async fn finalize_addition_uses_mono_synthdef_for_one_channel_port() {
        let (handler, backend, _state, voice_id, port_name, dest_group_id) =
            setup_voice_in_group(1).await;

        let mut diff = RouteDiff::default();
        diff.additions.push(Route {
            voice_id,
            port_name,
            dest: RouteDest::Group(dest_group_id),
        });

        handler.finalize(&diff).await.unwrap();

        let creates = backend.creates();
        assert_eq!(creates[0].def, "port_to_group_link_1");
    }

    #[tokio::test]
    async fn finalize_main_route_targets_bus_zero() {
        let (handler, backend, _state, voice_id, port_name, _dest_group) =
            setup_voice_in_group(2).await;

        let mut diff = RouteDiff::default();
        diff.additions.push(Route {
            voice_id,
            port_name,
            dest: RouteDest::Main,
        });

        handler.finalize(&diff).await.unwrap();

        let creates = backend.creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].out_bus, 0.0, "Main route writes to bus 0");
    }

    #[tokio::test]
    async fn finalize_muted_route_creates_no_mixer() {
        let (handler, backend, state, voice_id, port_name, _dest_group) =
            setup_voice_in_group(2).await;

        let mut diff = RouteDiff::default();
        diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Muted,
        });

        handler.finalize(&diff).await.unwrap();

        assert_eq!(backend.synths_created(), 0);
        assert_eq!(backend.nodes_freed(), 0);
        assert!(!state
            .read()
            .await
            .route_synths
            .contains_key(&(voice_id, port_name)));
    }

    #[tokio::test]
    async fn finalize_change_frees_old_and_creates_new_mixer() {
        // Move route from group A → group B: old mixer freed, new mixer instantiated.
        let (handler, backend, state, voice_id, port_name, dest_a) =
            setup_voice_in_group(2).await;

        // Add a second destination group B alongside A.
        let dest_b = GroupId::new(99);
        {
            let mut s = state.write().await;
            let node = s.alloc_node_id();
            let bus = s.alloc_audio_bus(2);
            s.groups.insert(
                dest_b,
                GroupState {
                    id: dest_b,
                    name: "B".to_string(),
                    parent: None,
                    node_id: node,
                    audio_bus: bus,
                    link_synth_node_id: None,
                    muted: false,
                    soloed: false,
                    params: ParamMap::new(),
                    output_bus: None,
                },
            );
        }

        // First, add a route to A.
        let mut add_diff = RouteDiff::default();
        add_diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_a),
        });
        handler.finalize(&add_diff).await.unwrap();
        let node_a = state
            .read()
            .await
            .route_synths
            .get(&(voice_id, port_name.clone()))
            .copied()
            .unwrap();

        // Now move: change A → B.
        let mut change_diff = RouteDiff::default();
        change_diff.changes.push(RouteChange {
            voice_id,
            port_name: port_name.clone(),
            old_dest: RouteDest::Group(dest_a),
            new_dest: RouteDest::Group(dest_b),
        });
        handler.finalize(&change_diff).await.unwrap();

        // Old node freed, new node created.
        assert!(backend.frees().contains(&node_a), "old mixer was freed");
        let creates = backend.creates();
        assert_eq!(creates.len(), 2, "one create on add, one on change");
        let bus_b = state
            .read()
            .await
            .groups
            .get(&dest_b)
            .unwrap()
            .audio_bus
            .0 as f32;
        assert_eq!(creates[1].out_bus, bus_b);

        // route_synths now points to the new node — its bus targets group B,
        // which proves we re-spawned rather than just rerouting an existing
        // synth. (The new node id may equal node_a if the freed id was
        // recycled by the allocator — that is intentional and harmless.)
        let node_after = state
            .read()
            .await
            .route_synths
            .get(&(voice_id, port_name))
            .copied()
            .unwrap();
        assert_eq!(
            node_after, creates[1].node,
            "route_synths key now tracks the second create"
        );
    }

    #[tokio::test]
    async fn finalize_removal_frees_mixer() {
        let (handler, backend, state, voice_id, port_name, dest) =
            setup_voice_in_group(2).await;

        let mut add_diff = RouteDiff::default();
        add_diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest),
        });
        handler.finalize(&add_diff).await.unwrap();

        let mut rm_diff = RouteDiff::default();
        rm_diff.removals.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest),
        });
        handler.finalize(&rm_diff).await.unwrap();

        assert_eq!(backend.nodes_freed(), 1);
        assert!(!state
            .read()
            .await
            .route_synths
            .contains_key(&(voice_id, port_name)));
    }

    #[tokio::test]
    async fn finalize_voice_delete_path_clears_all_owned_mixers() {
        // "Voice deleted: all owned mixer synths freed; no stale In.ar reads."
        // We simulate the voice-delete path by populating routes for two ports
        // on the same voice, then calling State::take_voice_route_nodes (which
        // is what VoicesHandler::delete invokes) and freeing on the backend.
        let (handler, backend, state, voice_id, _port_a, dest) =
            setup_voice_in_group(2).await;

        // Add a second port to the voice and a second route.
        {
            let mut s = state.write().await;
            let extra_bus = s.audio_buses.alloc(2);
            s.voices
                .get_mut(&voice_id)
                .unwrap()
                .output_buses
                .push(("aux".to_string(), extra_bus));
            // Update the synthdef registration so 'aux' has channels=2 too.
            s.synthdef_outputs.insert(
                "test_synth".to_string(),
                vec![
                    vibelang_dsp::OutputPort {
                        name: "out".to_string(),
                        channels: 2,
                        rate: vibelang_dsp::PortRate::Ar,
                    },
                    vibelang_dsp::OutputPort {
                        name: "aux".to_string(),
                        channels: 2,
                        rate: vibelang_dsp::PortRate::Ar,
                    },
                ],
            );
        }
        let mut add_diff = RouteDiff::default();
        add_diff.additions.push(Route {
            voice_id,
            port_name: "out".to_string(),
            dest: RouteDest::Group(dest),
        });
        add_diff.additions.push(Route {
            voice_id,
            port_name: "aux".to_string(),
            dest: RouteDest::Group(dest),
        });
        handler.finalize(&add_diff).await.unwrap();

        assert_eq!(state.read().await.route_synths.len(), 2);
        assert_eq!(backend.synths_created(), 2);

        // Voice-delete path: drain all route synths owned by this voice.
        let drained = {
            let mut s = state.write().await;
            s.take_voice_route_nodes(voice_id)
        };
        assert_eq!(drained.len(), 2, "both port mixers drained");
        assert!(state.read().await.route_synths.is_empty());

        for node in drained {
            backend.free_node(node).await.unwrap();
        }
        assert_eq!(backend.nodes_freed(), 2);
    }

    #[tokio::test]
    async fn finalize_no_changes_is_a_no_op() {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        handler.finalize(&RouteDiff::default()).await.unwrap();

        assert_eq!(backend.synths_created(), 0);
        assert_eq!(backend.nodes_freed(), 0);
    }
}
