//! Per-voice output routing.
//!
//! Each voice declares one or more output ports (named audio buses, allocated
//! by Story 2's [`VoiceState::output_buses`](crate::state::VoiceState::output_buses)).
//! A `RouteDest` says where a single port's signal should go: into a group's
//! mix bus, straight to main, or discarded.
//!
//! [`RoutesHandler::finalize`] instantiates a `port_to_group_link_<channels>`
//! mixer synth for each added route, frees the mixer for each removed route,
//! and swaps the mixer for changed routes.

use crate::backend::{AddAction, Backend};
use crate::compat::RwLock;
use crate::state::State;
use crate::types::{BusId, ControlBusId, GroupId, NodeId, ParamMap, VoiceId};
use crate::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use vibelang_dsp::OutputPort;

/// Where a voice's output port should send its audio (or, for kr ports,
/// its CV signal).
///
/// - `Group(id)` mixes into the named group's audio bus.
/// - `Main` sends directly to bus 0 (the hardware main output), bypassing groups.
/// - `Muted` discards the signal.
/// - `Param` maps a target voice's parameter to read from the source port's
///   control bus. Storage for multi-target Param routes uses [`ParamRouteMap`]
///   so one source port can drive params on multiple targets; this variant
///   exists in the enum for diff/API symmetry and is not stored as a value
///   in [`RouteMap`] (the route mixer-synth pipeline ignores it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RouteDest {
    Group(GroupId),
    Main,
    Muted,
    Param {
        voice_id: VoiceId,
        param_name: String,
    },
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

/// Multi-target Param routes: source `(voice_id, port_name)` →
/// list of `(target_voice_id, target_param_name)` pairs.
///
/// One source kr port can drive params on multiple target voices, so unlike
/// [`RouteMap`] the value is a list. Diffed by [`RoutesHandler::diff_params`]
/// and applied by [`RoutesHandler::finalize_params`], which issues `/n_map`
/// for each `(source_bus, target_param)` pair on the target voice's currently
/// active synth nodes. Mirrors [`crate::state::State::param_routes`] which
/// holds the *applied* baseline.
pub type ParamRouteMap = HashMap<(VoiceId, String), Vec<(VoiceId, String)>>;

/// A single Param route — one source kr port → one target param.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParamRoute {
    pub source_voice: VoiceId,
    pub source_port: String,
    pub target_voice: VoiceId,
    pub target_param: String,
}

/// Difference between two [`ParamRouteMap`]s.
///
/// Param routes don't have a Group/Main-style "change" axis: changing a
/// target's source port is just `removal(old) + addition(new)`. The diff is
/// computed at `(source, target)`-tuple granularity rather than per source
/// key, so a fan-out add or single-target removal both fall out naturally.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParamRouteDiff {
    pub additions: Vec<ParamRoute>,
    pub removals: Vec<ParamRoute>,
}

impl ParamRouteDiff {
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty()
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

/// A single planned action against one `(target_voice, target_param)` pair,
/// produced by [`RoutesHandler::plan_param_actions`] and consumed by
/// [`RoutesHandler::apply_param_action`].
struct PlannedParamAction {
    /// Kept for diagnostic logging — `target_nodes` carries the actual
    /// per-node dispatch list, so this isn't read on the hot path.
    #[allow(dead_code)]
    target_voice: VoiceId,
    target_param: String,
    /// Active synth nodes of the target voice that need their param remapped.
    target_nodes: Vec<NodeId>,
    plan: ParamPlan,
}

/// The action to take for a single target after fan-in reconciliation.
enum ParamPlan {
    /// No source remains — `/n_map node param -1` to revert the param.
    Unmap,
    /// Exactly one source — direct `/n_map` to the source's control bus.
    Direct(BusId),
    /// Two or more sources — spawn a `param_kr_sum_<n>` summer that writes
    /// to `intermediate_bus`, then `/n_map` the target to that bus.
    Summer {
        synthdef: String,
        summer_node: NodeId,
        target_group: NodeId,
        params: ParamMap,
        intermediate_bus: BusId,
    },
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
    /// 2. Instantiate mixer synths for *additions* and the new destination of
    ///    *changes*.
    ///
    /// Step 1 runs before step 2 so a moved route can reuse the same node-id
    /// pool slot; both steps drop the state lock before each backend call to
    /// preserve the project's lock discipline.
    pub async fn finalize(&self, diff: &RouteDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }

        let nodes_to_free: Vec<NodeId> = {
            let mut state = self.state.write().await;
            let mut nodes = Vec::new();
            let teardown_keys: Vec<(VoiceId, String)> = diff
                .removals
                .iter()
                .map(|r| (r.voice_id, r.port_name.clone()))
                .chain(
                    diff.changes
                        .iter()
                        .map(|c| (c.voice_id, c.port_name.clone())),
                )
                .collect();
            for key in teardown_keys {
                if let Some(node_id) = state.route_synths.remove(&key) {
                    state.free_node_id(node_id);
                    nodes.push(node_id);
                } else {
                    tracing::debug!(
                        "RoutesHandler::finalize: no live mixer for torn-down route voice={:?} port={:?} (already freed?)",
                        key.0,
                        key.1
                    );
                }
            }
            nodes
        };
        for node_id in nodes_to_free {
            tracing::debug!(
                "RoutesHandler::finalize: freeing route node {:?}",
                node_id
            );
            let _ = self.backend.free_node(node_id).await;
        }

        for r in &diff.additions {
            if let Err(e) = self
                .spawn_route(r.voice_id, &r.port_name, &r.dest)
                .await
            {
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

    /// Compute the additions and removals between two Param-route maps.
    ///
    /// Operates at `(source, target)`-tuple granularity: a source key whose
    /// target list shrinks contributes one removal per dropped target; a key
    /// whose target list grows contributes one addition per new target. There
    /// is no "change" axis (changing a target's bound source port is just a
    /// remove + add against different source keys).
    pub fn diff_params(old: &ParamRouteMap, new: &ParamRouteMap) -> ParamRouteDiff {
        let mut diff = ParamRouteDiff::default();

        for ((src_voice, src_port), new_targets) in new {
            let old_targets = old.get(&(*src_voice, src_port.clone()));
            for (tgt_voice, tgt_param) in new_targets {
                let in_old = old_targets
                    .map(|v| v.iter().any(|t| t.0 == *tgt_voice && t.1 == *tgt_param))
                    .unwrap_or(false);
                if !in_old {
                    diff.additions.push(ParamRoute {
                        source_voice: *src_voice,
                        source_port: src_port.clone(),
                        target_voice: *tgt_voice,
                        target_param: tgt_param.clone(),
                    });
                }
            }
        }

        for ((src_voice, src_port), old_targets) in old {
            let new_targets = new.get(&(*src_voice, src_port.clone()));
            for (tgt_voice, tgt_param) in old_targets {
                let in_new = new_targets
                    .map(|v| v.iter().any(|t| t.0 == *tgt_voice && t.1 == *tgt_param))
                    .unwrap_or(false);
                if !in_new {
                    diff.removals.push(ParamRoute {
                        source_voice: *src_voice,
                        source_port: src_port.clone(),
                        target_voice: *tgt_voice,
                        target_param: tgt_param.clone(),
                    });
                }
            }
        }

        diff
    }

    /// Apply a Param-route diff: tear down stale mappings, `/n_map` new ones,
    /// and reconcile multi-source kr fan-in via `param_kr_sum_<n>` summer
    /// synths where N>1 sources point at the same target param.
    ///
    /// Order:
    /// 1. Apply removals + additions to [`State::param_routes`] under one
    ///    lock so the desired source set per `(target_voice, target_param)`
    ///    is fully resolved before any backend call.
    /// 2. For each *affected* target (any target that appears in either side
    ///    of the diff), tear down its existing summer (if any), look up the
    ///    new source-bus set, and decide an action:
    ///      - **0 sources** → emit `/n_map node param -1` (scsynth unmap
    ///        sentinel) on every active node of the target voice, reverting
    ///        the param to its synthdef-declared default.
    ///      - **1 source** → direct `/n_map` from the surviving source's
    ///        control bus (legacy single-source path).
    ///      - **N ≥ 2 sources** → allocate an intermediate control bus +
    ///        `param_kr_sum_<N>` summer synth (placed at tail of the
    ///        modulator group so it runs after kr sources but before voice
    ///        groups), then `/n_map` to the intermediate bus.
    ///    Sources beyond [`PARAM_KR_SUM_MAX`](vibelang_dsp::system_synthdefs::PARAM_KR_SUM_MAX)
    ///    are truncated with a warning.
    /// 3. Affected targets are processed in diff order — removals first, then
    ///    additions, deduplicated — which preserves the legacy
    ///    "unmap-old-then-map-new" ordering for "change" diffs (a
    ///    `(source → target_a)` removal paired with a `(source → target_b)`
    ///    addition emits the unmap before the new map).
    ///
    /// All backend calls are issued outside the state lock to preserve the
    /// project's lock discipline.
    pub async fn finalize_params(&self, diff: &ParamRouteDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }

        // Plan everything under one lock: applies the diff to param_routes,
        // tears down old summer bookkeeping, computes per-target actions.
        let (planned, summers_to_free) = self.plan_param_actions(diff).await;

        // Free old summer synths on the backend first so any reused node IDs
        // don't collide with newly spawned summers below.
        for &(node, _) in &summers_to_free {
            if let Err(e) = self.backend.free_node(node).await {
                tracing::warn!(
                    "RoutesHandler::finalize_params: failed to free old summer node {:?}: {}",
                    node,
                    e,
                );
            }
        }
        if !summers_to_free.is_empty() {
            let mut state = self.state.write().await;
            for (node, bus) in summers_to_free {
                state.free_node_id(node);
                state.free_control_bus(bus);
            }
        }

        for action in planned {
            self.apply_param_action(action).await?;
        }

        Ok(())
    }

    /// Stage `state.param_routes` and `state.param_summers` against `diff`,
    /// returning the per-target actions to drive plus any summer nodes/buses
    /// to free on the backend afterwards.
    ///
    /// Pulled out of [`Self::finalize_params`] so the lock-held planning step
    /// is distinct from the lock-free backend dispatch step.
    async fn plan_param_actions(
        &self,
        diff: &ParamRouteDiff,
    ) -> (Vec<PlannedParamAction>, Vec<(NodeId, ControlBusId)>) {
        let mut state = self.state.write().await;

        // Apply removals to state.param_routes — drop the matching
        // `(target_voice, target_param)` from each source's target list,
        // and prune source keys whose target list goes empty.
        for r in &diff.removals {
            let src_key = (r.source_voice, r.source_port.clone());
            let mut empty_now = false;
            if let Some(targets) = state.param_routes.get_mut(&src_key) {
                targets.retain(|(tv, tp)| !(*tv == r.target_voice && *tp == r.target_param));
                empty_now = targets.is_empty();
            }
            if empty_now {
                state.param_routes.remove(&src_key);
            }
        }
        // Apply additions — push new target pairs onto the source's list,
        // skipping sources whose port doesn't exist on the source voice.
        for r in &diff.additions {
            let source_exists = state
                .voices
                .get(&r.source_voice)
                .map(|v| v.output_buses.iter().any(|(n, _)| *n == r.source_port))
                .unwrap_or(false);
            if !source_exists {
                tracing::warn!(
                    "RoutesHandler::finalize_params: source port {:?} not found on voice {:?}, skipping addition",
                    r.source_port,
                    r.source_voice,
                );
                continue;
            }
            let src_key = (r.source_voice, r.source_port.clone());
            let entry = state.param_routes.entry(src_key).or_default();
            let target_pair = (r.target_voice, r.target_param.clone());
            if !entry.iter().any(|t| *t == target_pair) {
                entry.push(target_pair);
            }
        }

        // Build a deterministic order of affected target keys: removals first
        // (so any change-diff emits the unmap before the remap), then
        // additions; deduplicate while preserving first-seen order.
        let mut seen: HashSet<(VoiceId, String)> = HashSet::new();
        let mut affected: Vec<(VoiceId, String)> = Vec::new();
        for r in diff.removals.iter().chain(diff.additions.iter()) {
            let key = (r.target_voice, r.target_param.clone());
            if seen.insert(key.clone()) {
                affected.push(key);
            }
        }

        let mut planned = Vec::with_capacity(affected.len());
        let mut summers_to_free: Vec<(NodeId, ControlBusId)> = Vec::new();

        for tgt in affected {
            // Tear down any existing summer for this target — even if the
            // new arity matches, we respawn so the parameter list reflects
            // the new source bus IDs.
            if let Some((node, bus)) = state.param_summers.remove(&tgt) {
                summers_to_free.push((node, ControlBusId::new(bus.raw())));
            }

            // Walk param_routes for every source whose target list contains
            // this target. Sort by source bus ID so the assigned `in_a`,
            // `in_b`, … positions are deterministic across runs.
            let mut source_buses: Vec<BusId> = Vec::new();
            for ((sv, sp), targets) in state.param_routes.iter() {
                if targets.iter().any(|t| t == &tgt) {
                    if let Some(bus) = state.voices.get(sv).and_then(|v| {
                        v.output_buses
                            .iter()
                            .find(|(n, _)| n == sp)
                            .map(|(_, b)| *b)
                    }) {
                        source_buses.push(bus);
                    }
                }
            }
            source_buses.sort_by_key(|b| b.raw());

            let target_nodes: Vec<NodeId> = state
                .voices
                .get(&tgt.0)
                .map(|v| {
                    v.active_nodes
                        .iter()
                        .copied()
                        .chain(v.note_nodes.values().copied())
                        .collect()
                })
                .unwrap_or_default();

            let plan = match source_buses.len() {
                0 => ParamPlan::Unmap,
                1 => ParamPlan::Direct(source_buses[0]),
                n => {
                    let max_n = vibelang_dsp::system_synthdefs::PARAM_KR_SUM_MAX;
                    let used: Vec<BusId> = if n > max_n {
                        tracing::warn!(
                            "RoutesHandler::finalize_params: {} kr sources point at target {:?} param {:?}, exceeds max {}; truncating",
                            n,
                            tgt.0,
                            tgt.1,
                            max_n,
                        );
                        source_buses.into_iter().take(max_n).collect()
                    } else {
                        source_buses
                    };
                    let arity = used.len();
                    let intermediate = state.alloc_control_bus();
                    let intermediate_bus = BusId::new(intermediate.raw());
                    let summer_node = state.alloc_node_id();
                    state
                        .param_summers
                        .insert(tgt.clone(), (summer_node, intermediate_bus));

                    // Place the summer in the modulator group when one
                    // exists (matches the runtime's "modulators run before
                    // voices" tick ordering); otherwise fall back to root —
                    // tests and minimal patches without explicit modulator
                    // groups still work, with the caveat that ordering
                    // against voice synths is implementation-defined.
                    let target_group = state
                        .modulator_group
                        .unwrap_or_else(|| NodeId::new(0));

                    let mut params = ParamMap::new();
                    let port_letters = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
                    for (i, src_bus) in used.iter().enumerate() {
                        params.insert(
                            format!("in_{}", port_letters[i]),
                            src_bus.raw() as f32,
                        );
                    }
                    params.insert("out_bus".to_string(), intermediate_bus.raw() as f32);

                    ParamPlan::Summer {
                        synthdef: format!("param_kr_sum_{}", arity),
                        summer_node,
                        target_group,
                        params,
                        intermediate_bus,
                    }
                }
            };

            planned.push(PlannedParamAction {
                target_voice: tgt.0,
                target_param: tgt.1,
                target_nodes,
                plan,
            });
        }

        (planned, summers_to_free)
    }

    /// Drive a single [`PlannedParamAction`] to the backend. Issues the
    /// matching `/s_new` (for summers) and `/n_map` calls.
    async fn apply_param_action(&self, action: PlannedParamAction) -> Result<()> {
        match action.plan {
            ParamPlan::Unmap => {
                for node in action.target_nodes {
                    // u32::MAX casts to -1i32 in the OSC `/n_map` payload —
                    // the scsynth unmap sentinel that reverts the param to
                    // its synthdef-declared default value.
                    if let Err(e) = self
                        .backend
                        .map_param_to_bus(node, &action.target_param, u32::MAX)
                        .await
                    {
                        tracing::warn!(
                            "RoutesHandler::finalize_params: unmap failed node={:?} param={:?}: {}",
                            node,
                            action.target_param,
                            e,
                        );
                    }
                }
            }
            ParamPlan::Direct(bus) => {
                for node in action.target_nodes {
                    self.backend
                        .map_param_to_bus(node, &action.target_param, bus.raw())
                        .await
                        .map_err(Error::backend)?;
                }
            }
            ParamPlan::Summer {
                synthdef,
                summer_node,
                target_group,
                params,
                intermediate_bus,
            } => {
                self.backend
                    .create_synth(
                        &synthdef,
                        summer_node,
                        target_group,
                        AddAction::Tail,
                        &params,
                    )
                    .await
                    .map_err(Error::backend)?;
                for node in action.target_nodes {
                    self.backend
                        .map_param_to_bus(node, &action.target_param, intermediate_bus.raw())
                        .await
                        .map_err(Error::backend)?;
                }
            }
        }
        Ok(())
    }

    /// Instantiate one mixer synth for `(voice, port) → dest`.
    ///
    /// `Muted` and `Param` destinations short-circuit (no mixer is created;
    /// `Param` is handled by [`Self::finalize_params`]). `Main` routes target
    /// bus 0 (hardware stereo out); `Group(id)` routes target the group's
    /// audio bus. The mixer synthdef variant (`port_to_group_link_1` vs `_2`)
    /// is chosen from the port's declared channel count.
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
                port_name,
            );
            return Ok(());
        }
        if matches!(dest, RouteDest::Param { .. }) {
            tracing::debug!(
                "RoutesHandler: Param route voice={:?} port={:?} — handled by finalize_params, no mixer synth",
                voice_id,
                port_name
            );
            return Ok(());
        }

        let (link_node, group_node, link_in_bus, channels, link_out_bus) = {
            let mut state = self.state.write().await;

            let voice = state
                .voices
                .get(&voice_id)
                .ok_or(Error::VoiceNotFound(voice_id))?;

            let port_bus = voice
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
                RouteDest::Muted | RouteDest::Param { .. } => {
                    unreachable!("filtered above")
                }
            };

            let link_node = state.alloc_node_id();
            state
                .route_synths
                .insert((voice_id, port_name.to_string()), link_node);

            (link_node, group_node, port_bus, channels, out_bus)
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
        params.insert("in_bus".to_string(), link_in_bus.0 as f32);
        params.insert("out_bus".to_string(), link_out_bus.0 as f32);

        tracing::debug!(
            "RoutesHandler: spawning {} (node={:?}) voice={:?} port={:?} in_bus={} out_bus={} group_node={:?}",
            synthdef_name,
            link_node,
            voice_id,
            port_name,
            link_in_bus.0,
            link_out_bus.0,
            group_node
        );

        self.backend
            .create_synth(
                synthdef_name,
                link_node,
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
    // Diff tests
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
        params: ParamMap,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct MapCall {
        node: NodeId,
        param: String,
        bus: u32,
    }

    struct MockBackend {
        synths_created: AtomicU32,
        nodes_freed: AtomicU32,
        last_creates: Mutex<Vec<CreateSynthCall>>,
        last_frees: Mutex<Vec<NodeId>>,
        last_maps: Mutex<Vec<MapCall>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                last_creates: Mutex::new(Vec::new()),
                last_frees: Mutex::new(Vec::new()),
                last_maps: Mutex::new(Vec::new()),
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
        fn maps(&self) -> Vec<MapCall> {
            self.last_maps.lock().unwrap().clone()
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
                params: params.clone(),
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
            node: NodeId,
            param: &str,
            bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            self.last_maps.lock().unwrap().push(MapCall {
                node,
                param: param.to_string(),
                bus,
            });
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


    // =========================================================================
    // Param-route diff + finalize_params (Multi-output v2 Story 3)
    // =========================================================================

    /// Build a state with two voices: a `source` voice owning a single kr
    /// output port (control bus) and a `target` voice with `active_synth_count`
    /// pre-populated `active_nodes`. Returns
    /// `(handler, backend, state, source_voice, source_port, target_voice,
    ///   target_param, source_control_bus, target_active_nodes)`.
    async fn setup_param_route_pair(
        active_synth_count: usize,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        VoiceId,
        String,
        VoiceId,
        String,
        BusId,
        Vec<NodeId>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let source_voice = VoiceId::new(10);
        let target_voice = VoiceId::new(20);
        let voice_group_id = GroupId::new(1);
        let source_port = "env".to_string();
        let target_param = "cutoff".to_string();

        let mut active_target_nodes = Vec::with_capacity(active_synth_count);
        let source_control_bus;

        {
            let mut s = state.write().await;

            s.synthdefs.insert("kr_synth".to_string());
            s.synthdef_outputs.insert(
                "kr_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: source_port.clone(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                }],
            );
            s.synthdefs.insert("ar_synth".to_string());
            s.synthdef_outputs.insert(
                "ar_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );

            let voice_group_node = s.alloc_node_id();
            let voice_group_bus = s.alloc_audio_bus(2);
            s.groups.insert(
                voice_group_id,
                GroupState {
                    id: voice_group_id,
                    name: "g".to_string(),
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

            // Source voice: kr port → one control bus.
            let kr_bus = s.alloc_control_bus();
            source_control_bus = BusId::new(kr_bus.raw());
            s.voices.insert(
                source_voice,
                VoiceState {
                    id: source_voice,
                    config: VoiceConfig::new("source", "kr_synth", voice_group_id),
                    active_nodes: Vec::new(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![(source_port.clone(), source_control_bus)],
                },
            );

            // Target voice: pre-populate `active_synth_count` active synth
            // node IDs so finalize_params has something to /n_map onto.
            for _ in 0..active_synth_count {
                active_target_nodes.push(s.alloc_node_id());
            }
            let target_audio_bus = s.alloc_audio_bus(2);
            s.voices.insert(
                target_voice,
                VoiceState {
                    id: target_voice,
                    config: VoiceConfig::new("target", "ar_synth", voice_group_id),
                    active_nodes: active_target_nodes.clone(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), target_audio_bus)],
                },
            );
        }

        (
            handler,
            backend,
            state,
            source_voice,
            source_port,
            target_voice,
            target_param,
            source_control_bus,
            active_target_nodes,
        )
    }

    fn make_param_map(
        entries: &[((u32, &str), &[(u32, &str)])],
    ) -> ParamRouteMap {
        entries
            .iter()
            .map(|((vid, port), targets)| {
                (
                    (VoiceId::new(*vid), (*port).to_string()),
                    targets
                        .iter()
                        .map(|(tv, tp)| (VoiceId::new(*tv), (*tp).to_string()))
                        .collect::<Vec<(VoiceId, String)>>(),
                )
            })
            .collect()
    }

    #[test]
    fn diff_params_empty_to_one_target_returns_one_addition() {
        let old = ParamRouteMap::new();
        let new = make_param_map(&[((10, "env"), &[(20, "cutoff")])]);
        let diff = RoutesHandler::<MockBackend>::diff_params(&old, &new);
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(diff.removals.len(), 0);
        assert_eq!(diff.additions[0].source_voice, VoiceId::new(10));
        assert_eq!(diff.additions[0].source_port, "env");
        assert_eq!(diff.additions[0].target_voice, VoiceId::new(20));
        assert_eq!(diff.additions[0].target_param, "cutoff");
    }

    #[test]
    fn diff_params_identical_returns_empty() {
        let routes = make_param_map(&[((10, "env"), &[(20, "cutoff"), (21, "amp")])]);
        let diff = RoutesHandler::<MockBackend>::diff_params(&routes, &routes);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_params_target_added_to_existing_source_is_an_addition() {
        // Same source key, target list grew by one — that one entry shows up
        // as an addition; the existing target stays untouched.
        let old = make_param_map(&[((10, "env"), &[(20, "cutoff")])]);
        let new = make_param_map(&[((10, "env"), &[(20, "cutoff"), (21, "amp")])]);
        let diff = RoutesHandler::<MockBackend>::diff_params(&old, &new);
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(diff.removals.len(), 0);
        assert_eq!(diff.additions[0].target_voice, VoiceId::new(21));
        assert_eq!(diff.additions[0].target_param, "amp");
    }

    #[test]
    fn diff_params_target_removed_from_existing_source_is_a_removal() {
        let old = make_param_map(&[((10, "env"), &[(20, "cutoff"), (21, "amp")])]);
        let new = make_param_map(&[((10, "env"), &[(20, "cutoff")])]);
        let diff = RoutesHandler::<MockBackend>::diff_params(&old, &new);
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.additions.len(), 0);
        assert_eq!(diff.removals[0].target_voice, VoiceId::new(21));
        assert_eq!(diff.removals[0].target_param, "amp");
    }

    #[tokio::test]
    async fn finalize_params_addition_maps_target_param_to_source_bus_on_active_node() {
        // Single Param route: mapping persists, target reads source bus.
        let (
            handler,
            backend,
            state,
            source_voice,
            source_port,
            target_voice,
            target_param,
            source_bus,
            target_nodes,
        ) = setup_param_route_pair(1).await;

        let mut diff = ParamRouteDiff::default();
        diff.additions.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice,
            target_param: target_param.clone(),
        });
        handler.finalize_params(&diff).await.unwrap();

        let maps = backend.maps();
        assert_eq!(maps.len(), 1, "one /n_map issued for one active node");
        assert_eq!(maps[0].node, target_nodes[0]);
        assert_eq!(maps[0].param, target_param);
        assert_eq!(maps[0].bus, source_bus.raw());

        let s = state.read().await;
        let key = (source_voice, source_port);
        let recorded = s.param_routes.get(&key).expect("source key tracked");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], (target_voice, target_param));
    }

    #[tokio::test]
    async fn finalize_params_multiple_targets_from_same_source() {
        // Multiple Param routes from same source: all targets see the source signal.
        let (handler, backend, state, source_voice, source_port, _, _, source_bus, _) =
            setup_param_route_pair(0).await;

        // Build two extra target voices, each with one active synth node.
        let target_a = VoiceId::new(100);
        let target_b = VoiceId::new(101);
        let (node_a, node_b);
        {
            let mut s = state.write().await;
            node_a = s.alloc_node_id();
            node_b = s.alloc_node_id();
            let bus_a = s.alloc_audio_bus(2);
            let bus_b = s.alloc_audio_bus(2);
            let group = s.voices.values().next().unwrap().config.group;
            s.voices.insert(
                target_a,
                VoiceState {
                    id: target_a,
                    config: VoiceConfig::new("ta", "ar_synth", group),
                    active_nodes: vec![node_a],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_a)],
                },
            );
            s.voices.insert(
                target_b,
                VoiceState {
                    id: target_b,
                    config: VoiceConfig::new("tb", "ar_synth", group),
                    active_nodes: vec![node_b],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_b)],
                },
            );
        }

        let mut diff = ParamRouteDiff::default();
        diff.additions.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice: target_a,
            target_param: "cutoff".to_string(),
        });
        diff.additions.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice: target_b,
            target_param: "amp".to_string(),
        });
        handler.finalize_params(&diff).await.unwrap();

        let maps = backend.maps();
        assert_eq!(maps.len(), 2, "one /n_map per (target voice, param) pair");
        assert!(maps
            .iter()
            .any(|m| m.node == node_a && m.param == "cutoff" && m.bus == source_bus.raw()));
        assert!(maps
            .iter()
            .any(|m| m.node == node_b && m.param == "amp" && m.bus == source_bus.raw()));

        let s = state.read().await;
        let recorded = s
            .param_routes
            .get(&(source_voice, source_port))
            .expect("source key tracked");
        assert_eq!(recorded.len(), 2, "both targets registered under the source");
    }

    #[tokio::test]
    async fn finalize_params_removal_unmaps_target_to_default() {
        // Removed route: target param returns to default (we issue the
        // scsynth unmap sentinel `/n_map node param -1`).
        let (
            handler,
            backend,
            state,
            source_voice,
            source_port,
            target_voice,
            target_param,
            _source_bus,
            target_nodes,
        ) = setup_param_route_pair(1).await;

        let route = ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice,
            target_param: target_param.clone(),
        };
        let mut add = ParamRouteDiff::default();
        add.additions.push(route.clone());
        handler.finalize_params(&add).await.unwrap();
        assert_eq!(backend.maps().len(), 1);

        let mut rm = ParamRouteDiff::default();
        rm.removals.push(route);
        handler.finalize_params(&rm).await.unwrap();

        let maps = backend.maps();
        assert_eq!(maps.len(), 2, "addition + unmap");
        let unmap = &maps[1];
        assert_eq!(unmap.node, target_nodes[0]);
        assert_eq!(unmap.param, target_param);
        assert_eq!(
            unmap.bus,
            u32::MAX,
            "removed Param emits the scsynth unmap sentinel (-1 as u32::MAX)",
        );

        let s = state.read().await;
        assert!(
            !s.param_routes.contains_key(&(source_voice, source_port)),
            "source key pruned when its target list goes empty",
        );
    }

    #[tokio::test]
    async fn finalize_params_changed_target_unmaps_old_and_maps_new() {
        // Story 3 spec: "Changed Param: unmap old + map new". A change at the
        // diff layer is a removal + addition pair; verify both are issued.
        let (
            handler,
            backend,
            state,
            source_voice,
            source_port,
            _,
            _,
            source_bus,
            _,
        ) = setup_param_route_pair(0).await;

        let target_a = VoiceId::new(100);
        let target_b = VoiceId::new(101);
        let (node_a, node_b);
        {
            let mut s = state.write().await;
            node_a = s.alloc_node_id();
            node_b = s.alloc_node_id();
            let bus_a = s.alloc_audio_bus(2);
            let bus_b = s.alloc_audio_bus(2);
            let group = s.voices.values().next().unwrap().config.group;
            s.voices.insert(
                target_a,
                VoiceState {
                    id: target_a,
                    config: VoiceConfig::new("ta", "ar_synth", group),
                    active_nodes: vec![node_a],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_a)],
                },
            );
            s.voices.insert(
                target_b,
                VoiceState {
                    id: target_b,
                    config: VoiceConfig::new("tb", "ar_synth", group),
                    active_nodes: vec![node_b],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_b)],
                },
            );
        }

        // First map source → A; then change to source → B.
        let mut add = ParamRouteDiff::default();
        add.additions.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice: target_a,
            target_param: "cutoff".to_string(),
        });
        handler.finalize_params(&add).await.unwrap();

        let mut chg = ParamRouteDiff::default();
        chg.removals.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice: target_a,
            target_param: "cutoff".to_string(),
        });
        chg.additions.push(ParamRoute {
            source_voice,
            source_port: source_port.clone(),
            target_voice: target_b,
            target_param: "cutoff".to_string(),
        });
        handler.finalize_params(&chg).await.unwrap();

        let maps = backend.maps();
        assert_eq!(maps.len(), 3, "addition, then unmap of A, then map of B");
        // First call: initial addition on A.
        assert_eq!(maps[0].node, node_a);
        assert_eq!(maps[0].bus, source_bus.raw());
        // Second call: unmap of A.
        assert_eq!(maps[1].node, node_a);
        assert_eq!(maps[1].bus, u32::MAX);
        // Third call: new mapping on B.
        assert_eq!(maps[2].node, node_b);
        assert_eq!(maps[2].bus, source_bus.raw());

        let s = state.read().await;
        let recorded = s
            .param_routes
            .get(&(source_voice, source_port))
            .expect("source key still present");
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0], (target_b, "cutoff".to_string()));
    }

    #[tokio::test]
    async fn finalize_params_voice_delete_drains_source_and_target_sides() {
        // Voice delete cleans both source-side AND target-side routes.
        // Build: source S has kr port. Three targets T0 / T1 / T2. S→T0
        // and S→T1 are param routes. Also another source S2 routes to T0.
        // Deleting T0 should:
        //   - remove the S→T0 entry from S's source list (target-side cleanup)
        //   - remove the S2→T0 entry similarly
        //   - leave S→T1 intact
        // Deleting S should:
        //   - drain S's entire entry (source-side cleanup) and surface the
        //     remaining (T1, *) tuples to the caller for unmap dispatch.
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let _handler: RoutesHandler<MockBackend> =
            RoutesHandler::new(backend.clone(), state.clone());

        let s_voice = VoiceId::new(1);
        let s2_voice = VoiceId::new(2);
        let t0 = VoiceId::new(10);
        let t1 = VoiceId::new(11);
        {
            let mut st = state.write().await;
            st.param_routes.insert(
                (s_voice, "env".to_string()),
                vec![
                    (t0, "cutoff".to_string()),
                    (t1, "amp".to_string()),
                ],
            );
            st.param_routes.insert(
                (s2_voice, "lfo".to_string()),
                vec![(t0, "freq".to_string())],
            );
        }

        // Delete T0 — target-side scrubbing only; no source key matches T0.
        let drained_t0 = {
            let mut st = state.write().await;
            st.take_voice_param_routes(t0)
        };
        assert!(
            drained_t0.is_empty(),
            "no source-side entries belong to target voice T0",
        );
        let s = state.read().await;
        assert_eq!(
            s.param_routes
                .get(&(s_voice, "env".to_string()))
                .map(|v| v.len()),
            Some(1),
            "S still routes to T1 only; T0 entry removed",
        );
        assert_eq!(
            s.param_routes.get(&(s_voice, "env".to_string())).unwrap()[0],
            (t1, "amp".to_string()),
        );
        assert!(
            !s.param_routes.contains_key(&(s2_voice, "lfo".to_string())),
            "S2's only target was T0; entry pruned to empty and removed",
        );
        drop(s);

        // Delete S — drains the source key, returns the remaining
        // `(target, param)` pairs to the caller.
        let drained_s = {
            let mut st = state.write().await;
            st.take_voice_param_routes(s_voice)
        };
        assert_eq!(drained_s.len(), 1, "one source key drained from S");
        assert_eq!(drained_s[0].0, (s_voice, "env".to_string()));
        assert_eq!(drained_s[0].1, vec![(t1, "amp".to_string())]);
        let s = state.read().await;
        assert!(
            s.param_routes.is_empty(),
            "all param-route bookkeeping is now drained",
        );
    }

    // =========================================================================
    // Multi-source kr fan-in (param_kr_sum_<n>) — multi-output v2
    // =========================================================================

    /// Build a fan-in fixture: `n` kr-source voices each owning one kr port,
    /// plus one target voice with one active synth node and one ar output.
    /// Returns the handler/backend/state and `(source_voices, source_buses,
    /// target_voice, target_param, target_node)`.
    async fn setup_fan_in_fixture(
        n: usize,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        Vec<VoiceId>,
        Vec<BusId>,
        VoiceId,
        String,
        NodeId,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let voice_group_id = GroupId::new(1);
        let target_voice = VoiceId::new(100);
        let target_param = "cutoff".to_string();
        let mut source_voices = Vec::with_capacity(n);
        let mut source_buses = Vec::with_capacity(n);
        let target_node;
        {
            let mut s = state.write().await;

            s.synthdefs.insert("kr_synth".to_string());
            s.synthdef_outputs.insert(
                "kr_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                }],
            );
            s.synthdefs.insert("ar_synth".to_string());
            s.synthdef_outputs.insert(
                "ar_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );

            let voice_group_node = s.alloc_node_id();
            let voice_group_bus = s.alloc_audio_bus(2);
            s.groups.insert(
                voice_group_id,
                GroupState {
                    id: voice_group_id,
                    name: "g".to_string(),
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

            for i in 0..n {
                let vid = VoiceId::new(10 + i as u32);
                let kr_bus = s.alloc_control_bus();
                let bus = BusId::new(kr_bus.raw());
                s.voices.insert(
                    vid,
                    VoiceState {
                        id: vid,
                        config: VoiceConfig::new(
                            &format!("src{}", i),
                            "kr_synth",
                            voice_group_id,
                        ),
                        active_nodes: Vec::new(),
                        note_nodes: HashMap::new(),
                        round_robin_position: 0,
                        pending_params: HashMap::new(),
                        output_buses: vec![("out".to_string(), bus)],
                    },
                );
                source_voices.push(vid);
                source_buses.push(bus);
            }

            target_node = s.alloc_node_id();
            let target_audio_bus = s.alloc_audio_bus(2);
            s.voices.insert(
                target_voice,
                VoiceState {
                    id: target_voice,
                    config: VoiceConfig::new("target", "ar_synth", voice_group_id),
                    active_nodes: vec![target_node],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), target_audio_bus)],
                },
            );
        }

        (
            handler,
            backend,
            state,
            source_voices,
            source_buses,
            target_voice,
            target_param,
            target_node,
        )
    }

    /// Push a `(source → target)` addition onto a diff for every source.
    fn add_routes_for(
        diff: &mut ParamRouteDiff,
        sources: &[VoiceId],
        target_voice: VoiceId,
        target_param: &str,
    ) {
        for sv in sources {
            diff.additions.push(ParamRoute {
                source_voice: *sv,
                source_port: "out".to_string(),
                target_voice,
                target_param: target_param.to_string(),
            });
        }
    }

    #[tokio::test]
    async fn finalize_params_two_sources_spawn_summer_2_and_map_to_intermediate() {
        // Two kr sources to one target param: a single param_kr_sum_2 synth
        // is spawned, the intermediate control bus is allocated from the
        // free list, and the target's /n_map binds to that intermediate bus
        // (not to either source bus directly).
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_node) =
            setup_fan_in_fixture(2).await;

        let mut diff = ParamRouteDiff::default();
        add_routes_for(&mut diff, &sources, target_voice, &target_param);

        handler.finalize_params(&diff).await.unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_sum_2")
            .collect();
        assert_eq!(summers.len(), 1, "exactly one summer synth created");
        let summer = summers[0];
        // in_a + in_b are the two source buses (sorted by bus id internally).
        let mut sorted_sources: Vec<u32> = source_buses.iter().map(|b| b.raw()).collect();
        sorted_sources.sort();
        assert_eq!(*summer.params.get("in_a").unwrap() as u32, sorted_sources[0]);
        assert_eq!(*summer.params.get("in_b").unwrap() as u32, sorted_sources[1]);
        let intermediate = *summer.params.get("out_bus").unwrap() as u32;
        // Intermediate must come from the control-bus allocator (>= 1000).
        assert!(intermediate >= 1000);

        let maps = backend.maps();
        assert_eq!(maps.len(), 1, "one /n_map for the lone target node");
        assert_eq!(maps[0].node, target_node);
        assert_eq!(maps[0].param, target_param);
        assert_eq!(maps[0].bus, intermediate);

        let s = state.read().await;
        let recorded = s
            .param_summers
            .get(&(target_voice, target_param.clone()))
            .expect("summer tracked in param_summers");
        assert_eq!(recorded.0, summer.node);
        assert_eq!(recorded.1.raw(), intermediate);
        // Both source-side entries are recorded.
        let total_targets: usize = s
            .param_routes
            .values()
            .map(|t| t.len())
            .sum();
        assert_eq!(total_targets, 2);
    }

    #[tokio::test]
    async fn finalize_params_three_sources_spawn_summer_3() {
        // Three kr sources → param_kr_sum_3 with in_a/in_b/in_c populated.
        let (handler, backend, _state, sources, source_buses, target_voice, target_param, _) =
            setup_fan_in_fixture(3).await;

        let mut diff = ParamRouteDiff::default();
        add_routes_for(&mut diff, &sources, target_voice, &target_param);

        handler.finalize_params(&diff).await.unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_sum_3")
            .collect();
        assert_eq!(summers.len(), 1);
        let summer = summers[0];
        let mut sorted_sources: Vec<u32> = source_buses.iter().map(|b| b.raw()).collect();
        sorted_sources.sort();
        assert_eq!(*summer.params.get("in_a").unwrap() as u32, sorted_sources[0]);
        assert_eq!(*summer.params.get("in_b").unwrap() as u32, sorted_sources[1]);
        assert_eq!(*summer.params.get("in_c").unwrap() as u32, sorted_sources[2]);
    }

    #[tokio::test]
    async fn finalize_params_remove_one_of_three_respawns_summer_2() {
        // 3 sources → 2 sources: old _3 freed (node + intermediate bus
        // returned to the pool), new _2 spawned with the surviving sources,
        // target's /n_map points at the new intermediate bus.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_node) =
            setup_fan_in_fixture(3).await;

        let mut add = ParamRouteDiff::default();
        add_routes_for(&mut add, &sources, target_voice, &target_param);
        handler.finalize_params(&add).await.unwrap();

        let (old_summer_node, old_intermediate) = {
            let s = state.read().await;
            *s.param_summers
                .get(&(target_voice, target_param.clone()))
                .unwrap()
        };

        // Drop the first source. Affected target = (target_voice, target_param).
        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target_voice,
            target_param: target_param.clone(),
        });
        handler.finalize_params(&rm).await.unwrap();

        // Old summer node must have been freed.
        let frees = backend.frees();
        assert!(
            frees.contains(&old_summer_node),
            "old summer node freed on respawn",
        );

        // New summer is param_kr_sum_2.
        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> =
            creates.iter().filter(|c| c.def.starts_with("param_kr_sum_")).collect();
        assert_eq!(summers.len(), 2, "one summer per finalize call");
        assert_eq!(summers[0].def, "param_kr_sum_3");
        assert_eq!(summers[1].def, "param_kr_sum_2");

        // Target now bound to the *new* intermediate bus.
        let new_intermediate =
            *summers[1].params.get("out_bus").unwrap() as u32;
        let maps = backend.maps();
        let last_map = maps.last().unwrap();
        assert_eq!(last_map.node, target_node);
        assert_eq!(last_map.bus, new_intermediate);

        // The old intermediate bus should have returned to the free list, so
        // a fresh alloc reuses it before reaching for a brand-new ID.
        let mut s = state.write().await;
        let reused = s.alloc_control_bus().raw();
        assert_eq!(reused, old_intermediate.raw(), "intermediate bus recycled");
    }

    #[tokio::test]
    async fn finalize_params_remove_to_one_source_falls_back_to_direct_map() {
        // 2 sources → 1 source: summer + intermediate freed, /n_map binds
        // directly to the surviving source's control bus.
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_node) =
            setup_fan_in_fixture(2).await;

        let mut add = ParamRouteDiff::default();
        add_routes_for(&mut add, &sources, target_voice, &target_param);
        handler.finalize_params(&add).await.unwrap();
        let (summer_node, _) = {
            let s = state.read().await;
            *s.param_summers
                .get(&(target_voice, target_param.clone()))
                .unwrap()
        };

        // Pick the source with the larger bus id so it's the deterministic
        // survivor regardless of allocator order (we always remove sources[0]).
        let surviving_source = sources[1];
        let surviving_bus = source_buses[1];
        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target_voice,
            target_param: target_param.clone(),
        });
        handler.finalize_params(&rm).await.unwrap();

        // Old summer freed.
        assert!(backend.frees().contains(&summer_node));

        // No new summer spawned in this finalize.
        let creates = backend.creates();
        let new_summers: Vec<&CreateSynthCall> = creates
            .iter()
            .skip_while(|c| c.def != "param_kr_sum_2")
            .skip(1)
            .filter(|c| c.def.starts_with("param_kr_sum_"))
            .collect();
        assert!(
            new_summers.is_empty(),
            "no fresh summer when collapsing to a single source",
        );

        // Target /n_map redirected to the surviving source's bus.
        let last_map = backend.maps().last().cloned().unwrap();
        assert_eq!(last_map.node, target_node);
        assert_eq!(last_map.bus, surviving_bus.raw());

        // param_summers entry pruned.
        let s = state.read().await;
        assert!(!s
            .param_summers
            .contains_key(&(target_voice, target_param.clone())));
        // Source-side bookkeeping: only the surviving source's key remains.
        assert_eq!(s.param_routes.len(), 1);
        assert!(s
            .param_routes
            .contains_key(&(surviving_source, "out".to_string())));
    }

    #[tokio::test]
    async fn finalize_params_remove_all_sources_unmaps() {
        // 2 sources → 0 sources: summer freed and target unmapped.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_node) =
            setup_fan_in_fixture(2).await;

        let mut add = ParamRouteDiff::default();
        add_routes_for(&mut add, &sources, target_voice, &target_param);
        handler.finalize_params(&add).await.unwrap();

        let mut rm = ParamRouteDiff::default();
        for sv in &sources {
            rm.removals.push(ParamRoute {
                source_voice: *sv,
                source_port: "out".to_string(),
                target_voice,
                target_param: target_param.clone(),
            });
        }
        handler.finalize_params(&rm).await.unwrap();

        let last_map = backend.maps().last().cloned().unwrap();
        assert_eq!(last_map.node, target_node);
        assert_eq!(
            last_map.bus,
            u32::MAX,
            "target unmapped via the scsynth -1 sentinel",
        );

        let s = state.read().await;
        assert!(s.param_routes.is_empty());
        assert!(s.param_summers.is_empty());
    }

    #[tokio::test]
    async fn finalize_params_add_second_source_upgrades_to_summer() {
        // Existing single-source target: adding a second source spawns a
        // summer and remaps the target onto the intermediate bus. Even
        // though the second-source addition is the only route in the diff,
        // the planner must enumerate the *full* source set after applying
        // the diff (i.e. the pre-existing source must contribute to the
        // summer's input list).
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_node) =
            setup_fan_in_fixture(2).await;

        // First, install the single-source baseline.
        let mut step1 = ParamRouteDiff::default();
        step1.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target_voice,
            target_param: target_param.clone(),
        });
        handler.finalize_params(&step1).await.unwrap();

        // No summer yet; direct /n_map to source[0]'s bus.
        {
            let s = state.read().await;
            assert!(s.param_summers.is_empty());
        }
        let direct_map = backend.maps().last().cloned().unwrap();
        assert_eq!(direct_map.bus, source_buses[0].raw());

        // Now add the second source.
        let mut step2 = ParamRouteDiff::default();
        step2.additions.push(ParamRoute {
            source_voice: sources[1],
            source_port: "out".to_string(),
            target_voice,
            target_param: target_param.clone(),
        });
        handler.finalize_params(&step2).await.unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> =
            creates.iter().filter(|c| c.def == "param_kr_sum_2").collect();
        assert_eq!(summers.len(), 1);
        let intermediate = *summers[0].params.get("out_bus").unwrap() as u32;
        let last_map = backend.maps().last().cloned().unwrap();
        assert_eq!(last_map.node, target_node);
        assert_eq!(last_map.bus, intermediate);
    }

    #[tokio::test]
    async fn finalize_params_no_op_diff_preserves_existing_summer() {
        // Hot-reload signal: same source set, same target — diff is empty,
        // finalize short-circuits, the summer node + intermediate bus
        // outlive the call. (Equality at the bus-ID level guarantees the
        // backend keeps reading from the same intermediate.)
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, _) =
            setup_fan_in_fixture(2).await;

        let mut diff = ParamRouteDiff::default();
        add_routes_for(&mut diff, &sources, target_voice, &target_param);
        handler.finalize_params(&diff).await.unwrap();

        let snapshot = {
            let s = state.read().await;
            s.param_summers.clone()
        };
        let creates_before = backend.creates().len();
        let frees_before = backend.frees().len();

        // Simulate hot-reload with identical routes — diff comes back empty.
        let empty_diff = ParamRouteDiff::default();
        handler.finalize_params(&empty_diff).await.unwrap();

        let s = state.read().await;
        assert_eq!(s.param_summers, snapshot, "summer untouched on no-op diff");
        assert_eq!(backend.creates().len(), creates_before);
        assert_eq!(backend.frees().len(), frees_before);
    }
}
