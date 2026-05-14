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
use crate::state::{ParamSummerSource, ParamSummerState, State};
use crate::types::{BusId, ControlBusId, EffectId, GroupId, NodeId, ParamMap, VoiceId};
use crate::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use vibelang_dsp::{InputPort, OutputPort, PortRate};

/// Target side of a CV-to-param route.
///
/// A route's source is always a Voice (only voices own kr/tr/ar output ports),
/// but the target can be either another Voice's param or an Effect's param.
/// Voice-target routes resolve to every active voice node (`active_nodes` +
/// `note_nodes`); fx-target routes resolve to the effect's single
/// [`crate::state::EffectState::node_id`]. The `/n_map` pipeline doesn't
/// care which kind of node it's binding to — same backend call either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamRouteTarget {
    Voice(VoiceId),
    Effect(EffectId),
}

impl From<VoiceId> for ParamRouteTarget {
    fn from(v: VoiceId) -> Self {
        Self::Voice(v)
    }
}

impl From<EffectId> for ParamRouteTarget {
    fn from(e: EffectId) -> Self {
        Self::Effect(e)
    }
}

impl ParamRouteTarget {
    /// Convenience: `true` when this target is a Voice.
    pub fn is_voice(&self) -> bool {
        matches!(self, Self::Voice(_))
    }

    /// Convenience: `true` when this target is an Effect.
    pub fn is_effect(&self) -> bool {
        matches!(self, Self::Effect(_))
    }
}

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
/// The value is the list of destinations a single port fans out to. The
/// invariant kept by [`crate::reload::ScriptState::set_route`] is that the
/// list contains either exactly one [`RouteDest::Main`], exactly one
/// [`RouteDest::Muted`], or one or more [`RouteDest::Group`] entries
/// (deduplicated). Multi-target fan-out is supported only for
/// [`RouteDest::Group`] — `Main` is the hardware bus and `Muted` is silence,
/// so they keep replace semantics.
///
/// Stored on [`ScriptState`](crate::reload::ScriptState) so script-side mutations
/// can carry the desired route map across reloads.
pub type RouteMap = HashMap<(VoiceId, String), Vec<RouteDest>>;

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
) -> Vec<(String, Vec<RouteDest>)> {
    let count = match ports.len() {
        0 => 0,
        1 | 2 => ports.len(),
        _ => 2,
    };
    ports
        .iter()
        .take(count)
        .map(|p| (p.name.clone(), vec![RouteDest::Group(voice_group)]))
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
    for (key, dests) in user {
        merged.insert(key.clone(), dests.clone());
    }
    merged
}

/// Drop count-based default audio routes for voices that are used purely as
/// modulation sources.
///
/// Two activation paths:
///
/// 1. **Heuristic** — a voice qualifies as "modulation-only" when (a) the
///    script installed *no* explicit user routes against any of its ports
///    (`user_routes` carries no `(voice, *)` key) and (b) at least one of
///    the voice's ports appears as a *source* in any of the param-route
///    maps (SET via `.to_param` / `.to_param_audio`, BEND via
///    `.modulate_by`, or TRIGGER via `.to_trigger`). For such voices, the
///    implicit default `Group(voice_group)` mix would dump the voice's raw
///    waveform into the surrounding group's audio bus — audible bleed for
///    an LFO whose only purpose is modulating another voice's param.
///
/// 2. **Explicit flag** — `is_modulator_only_fn(vid)` returning true forces
///    suppression regardless of the heuristic conditions, even when an
///    explicit `RouteDest::{Group,Main,Muted}` user route is present and
///    even when there are no outgoing param routes. The user's explicit
///    routes still apply via [`merge_default_routes`]; only the implicit
///    default is dropped. This is the escape hatch for voices that legit
///    have both a wet audio destination (e.g. a recording group) and a
///    modulation role, where the heuristic's "any explicit route disables
///    me" rule would leave the implicit group mix in place.
///
/// The check is per-voice (not per-port): the typical case is a
/// single-output LFO synthdef whose one port is being used for modulation.
///
/// `voice_name_fn` is used purely for the `tracing::info!` line emitted on
/// activation (one per suppressed voice).
pub fn suppress_modulation_only_defaults(
    defaults: &RouteMap,
    user_routes: &RouteMap,
    set: &ParamRouteMap,
    bend: &ParamRouteMap,
    trigger: &ParamRouteMap,
    voice_name_fn: impl Fn(VoiceId) -> Option<String>,
    is_modulator_only_fn: impl Fn(VoiceId) -> bool,
) -> RouteMap {
    let voices_with_user_route: HashSet<VoiceId> =
        user_routes.keys().map(|(v, _)| *v).collect();

    let mut param_route_counts: HashMap<VoiceId, usize> = HashMap::new();
    for map in [set, bend, trigger] {
        for ((vid, _), targets) in map {
            *param_route_counts.entry(*vid).or_insert(0) += targets.len();
        }
    }

    let mut suppressed: HashSet<VoiceId> = HashSet::new();
    let mut suppressed_via_flag: HashSet<VoiceId> = HashSet::new();
    let mut filtered = RouteMap::new();
    for (key, dests) in defaults {
        let (vid, _) = key;
        let has_param = param_route_counts.contains_key(vid);
        let has_user = voices_with_user_route.contains(vid);
        let explicit_flag = is_modulator_only_fn(*vid);
        if explicit_flag || (has_param && !has_user) {
            suppressed.insert(*vid);
            if explicit_flag {
                suppressed_via_flag.insert(*vid);
            }
        } else {
            filtered.insert(key.clone(), dests.clone());
        }
    }

    for vid in &suppressed {
        let count = param_route_counts.get(vid).copied().unwrap_or(0);
        let name = voice_name_fn(*vid).unwrap_or_else(|| format!("{:?}", vid));
        let reason = if suppressed_via_flag.contains(vid) {
            "explicit modulator_only() flag"
        } else {
            "modulation-only"
        };
        tracing::info!(
            "Voice '{}': skipping default audio routing — {} ({} outgoing param routes)",
            name,
            reason,
            count
        );
    }

    filtered
}

/// Difference between two route maps, computed at `(voice, port, dest)`-edge
/// granularity.
///
/// Each route entry can fan out to multiple destinations (see [`RouteMap`]),
/// so the diff is per-edge rather than per-key: dropping one of N group
/// destinations on a port surfaces as a single removal, not a key-level
/// change. Re-pointing a port from `Main` → `Group(g)` is therefore a
/// removal of the old edge plus an addition of the new edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteDiff {
    /// Edges present in `new` but not in `old`.
    pub additions: Vec<Route>,
    /// Edges present in `old` but not in `new`.
    pub removals: Vec<Route>,
}

impl RouteDiff {
    /// True when there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty()
    }
}

/// Multi-target Param routes: source `(voice_id, port_name)` →
/// list of `(target, target_param_name)` pairs.
///
/// One source kr port can drive params on multiple target voices and/or
/// effects, so unlike [`RouteMap`] the value is a list. The target side is a
/// [`ParamRouteTarget`] enum so a single source key can fan out to any mix of
/// voice and fx targets. The map is shared between the SET pipeline
/// (`.to_param`) and the BEND pipeline (`.modulate_by`) — each pipeline owns
/// its own [`ParamRouteMap`] in [`crate::state::State::param_routes_set`] and
/// [`crate::state::State::param_routes_bend`]. Diffed by
/// [`RoutesHandler::diff_params`] and applied by
/// [`RoutesHandler::finalize_params`].
pub type ParamRouteMap = HashMap<(VoiceId, String), Vec<(ParamRouteTarget, String)>>;

/// A single Param route — one source kr port → one target param.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ParamRoute {
    pub source_voice: VoiceId,
    pub source_port: String,
    pub target: ParamRouteTarget,
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

/// Where a voice's named input port should read its signal from.
///
/// Structurally symmetric to [`RouteDest`] for outputs, but the variants
/// describe the *source* side of an input edge:
///
/// - `Voice(vid, port_name)` reads from another voice's output port bus.
/// - `Group(gid)` reads from a group's mix bus (the same audio bus that
///   `RouteDest::Group` writes into on the output side).
/// - `HardwareInput(channels)` reads from one or more hardware input
///   channels. Encoded as a `Vec<u32>` so a stereo or multichannel input
///   is one source rather than N fan-in edges. The interpretation of the
///   channel list is left to the P3.3 dispatcher; this type is pure data.
/// - `Silent` is an explicit "no source" source; the dispatcher resolves it
///   to the shared silent audio/control bus from P1.4 (see the named-inputs
///   design notes, decision #2: every declared input port has a valid bus
///   even when the script does not route anything into it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InputRouteSrc {
    Voice(VoiceId, String),
    Group(GroupId),
    HardwareInput(Vec<u32>),
    Silent,
}

/// A single concrete input route: one voice's named input port ← one source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InputRoute {
    pub voice_id: VoiceId,
    pub port_name: String,
    pub src: InputRouteSrc,
}

/// Per-voice input-route registry, keyed by `(target_voice_id, input_port_name)`.
///
/// The value is the list of sources feeding a single input port. Per the
/// named-inputs design notes (decision #1), the script API surface keeps
/// source ownership explicit: `voice.input("name").from(x)` replaces (Vec
/// length 1), and `voice.input("name").from_all([…])` /
/// `voice.input("name").add_from(x)` are the only routes that produce a Vec
/// with length > 1 (fan-in). The Vec also retains insertion order from the
/// script side so the eventual mixer-synth arity matches the source list.
///
/// Stored on [`crate::state::State::input_routes`] (the runtime side; the
/// script-side mirror lives on `ScriptState` once P2 lands).
pub type InputRouteMap = HashMap<(VoiceId, String), Vec<InputRouteSrc>>;

/// Difference between two [`InputRouteMap`]s, computed at
/// `(target_voice, input_port, source)`-edge granularity.
///
/// Mirrors [`RouteDiff`] for outputs: dropping one of N sources on a fan-in
/// port surfaces as a single removal, growing a Vec from length 1→2 is a
/// single addition. Re-pointing a single-source input from `Voice(a, "out")`
/// to `Group(g)` is removal of the old edge plus addition of the new edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputRouteDiff {
    pub additions: Vec<InputRoute>,
    pub removals: Vec<InputRoute>,
}

impl InputRouteDiff {
    /// True when there is nothing to apply.
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.removals.is_empty()
    }
}

/// Stable sort key for an [`InputRouteSrc`]. Tags fix variant order
/// (`Voice < Group < HardwareInput < Silent`); within each variant the
/// inner data orders lexicographically.
fn input_src_sort_key(src: &InputRouteSrc) -> (u8, u32, String, Vec<u32>) {
    match src {
        InputRouteSrc::Voice(vid, port) => (0, vid.raw(), port.clone(), Vec::new()),
        InputRouteSrc::Group(gid) => (1, gid.raw(), String::new(), Vec::new()),
        InputRouteSrc::HardwareInput(channels) => {
            (2, 0, String::new(), channels.clone())
        }
        InputRouteSrc::Silent => (3, 0, String::new(), Vec::new()),
    }
}

/// Compute the additions and removals between two [`InputRouteMap`]s at
/// `(target_voice, input_port, source)`-edge granularity.
///
/// Symmetric to [`RoutesHandler::diff`] on the output side: per-edge so a
/// fan-in delta (Vec length 1→2) surfaces as one addition and a fan-in drop
/// (2→1) as one removal. The output is sorted by
/// `(target_voice_id, input_port_name, source_sort_key)` so callers get a
/// deterministic order regardless of `HashMap` iteration order.
///
/// Pure function with no scsynth interaction — bus resolution and mixer-synth
/// allocation live in P3.3.
pub fn compute_input_route_diff(
    old: &InputRouteMap,
    new: &InputRouteMap,
) -> InputRouteDiff {
    let mut diff = InputRouteDiff::default();

    for ((voice_id, port_name), new_srcs) in new {
        let old_srcs = old.get(&(*voice_id, port_name.clone()));
        for s in new_srcs {
            let in_old = old_srcs
                .map(|v| v.iter().any(|x| x == s))
                .unwrap_or(false);
            if !in_old {
                diff.additions.push(InputRoute {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    src: s.clone(),
                });
            }
        }
    }

    for ((voice_id, port_name), old_srcs) in old {
        let new_srcs = new.get(&(*voice_id, port_name.clone()));
        for s in old_srcs {
            let in_new = new_srcs
                .map(|v| v.iter().any(|x| x == s))
                .unwrap_or(false);
            if !in_new {
                diff.removals.push(InputRoute {
                    voice_id: *voice_id,
                    port_name: port_name.clone(),
                    src: s.clone(),
                });
            }
        }
    }

    let sort_edges = |edges: &mut Vec<InputRoute>| {
        edges.sort_by(|a, b| {
            a.voice_id
                .raw()
                .cmp(&b.voice_id.raw())
                .then_with(|| a.port_name.cmp(&b.port_name))
                .then_with(|| input_src_sort_key(&a.src).cmp(&input_src_sort_key(&b.src)))
        });
    };
    sort_edges(&mut diff.additions);
    sort_edges(&mut diff.removals);

    diff
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

/// One `a2k_adapter_1` synth pending creation on the backend.
///
/// Produced by [`RoutesHandler::plan_param_actions`] and consumed by
/// [`RoutesHandler::finalize_params`] before any new summer is spawned, so
/// summers can read from the adapter's intermediate bus on their first tick.
struct AdapterSpawn {
    node: NodeId,
    in_bus: BusId,
    out_bus: ControlBusId,
}

struct InputRouteSpawn {
    synthdef: &'static str,
    node: NodeId,
    target_group: NodeId,
    in_bus: BusId,
    out_bus: BusId,
    target_param: String,
    target_nodes: Vec<NodeId>,
}

/// A single planned action against one `(target, target_param)` pair,
/// produced by [`RoutesHandler::plan_param_actions`] and consumed by
/// [`RoutesHandler::apply_param_action`].
struct PlannedParamAction {
    /// Kept for diagnostic logging — `target_nodes` carries the actual
    /// per-node dispatch list, so this isn't read on the hot path.
    #[allow(dead_code)]
    target: ParamRouteTarget,
    target_param: String,
    /// Active synth nodes of the target that need their param remapped.
    /// Voice targets contribute their `active_nodes` + `note_nodes`; fx
    /// targets contribute their single [`crate::state::EffectState::node_id`].
    target_nodes: Vec<NodeId>,
    plan: ParamPlan,
}

/// The action to take for a single target after route reconciliation.
enum ParamPlan {
    /// No source remains — `/n_map node param -1` (the scsynth unmap
    /// sentinel) on every active synth node, *and* `/n_set node param value`
    /// where `value` is the target voice's current `set_param` baseline so
    /// the param falls back to the user's value (not the synthdef default).
    Unmap { restore_value: Option<f32> },
    /// Spawn a `param_kr_modulate_<n>` summer and bind the target's
    /// `/n_map` to the summer's intermediate bus.
    ///
    /// Multi-output v3 unified routing: SET and BEND share this path. SET
    /// pins `baseline=0` (so the source carries through unchanged at the
    /// default scale=1/offset=0); BEND seeds `baseline` from the user's
    /// last `set_param` value so modulators bend around it.
    Summer {
        synthdef: String,
        summer_node: NodeId,
        target_group: NodeId,
        params: ParamMap,
        intermediate_bus: BusId,
    },
    /// Spawn a `port_tr_to_param_link_1` link synth that 1:1 forwards the
    /// source's Tr-rate bus to an intermediate kr bus, and bind the target's
    /// `/n_map` to that bus. No scale/offset shaping — triggers don't bend.
    /// Multi-output v3 B2.c trigger path.
    TriggerLink {
        link_node: NodeId,
        target_group: NodeId,
        params: ParamMap,
        intermediate_bus: BusId,
    },
}

/// Which paramroute map a diff applies to.
#[derive(Copy, Clone, Debug)]
enum ParamMapKind {
    Set,
    Bend,
    Trigger,
}

/// Stage a [`ParamRouteDiff`] against the matching map on the locked state.
///
/// Drops removed `(target, target_param)` from each source's target list
/// (pruning empty source keys), then appends additions, skipping any addition
/// whose source port is no longer registered on the source voice.
fn apply_param_diff_to_map(
    state: &mut State,
    diff: &ParamRouteDiff,
    kind: ParamMapKind,
) {
    let map: &mut ParamRouteMap = match kind {
        ParamMapKind::Set => &mut state.param_routes_set,
        ParamMapKind::Bend => &mut state.param_routes_bend,
        ParamMapKind::Trigger => &mut state.param_routes_trigger,
    };

    for r in &diff.removals {
        let src_key = (r.source_voice, r.source_port.clone());
        let mut empty_now = false;
        if let Some(targets) = map.get_mut(&src_key) {
            targets.retain(|(t, tp)| !(*t == r.target && *tp == r.target_param));
            empty_now = targets.is_empty();
        }
        if empty_now {
            map.remove(&src_key);
        }
    }

    // Source-port existence check needs the voices map, which lives on the
    // same `state` borrow — extract source info first to avoid double-borrow.
    let mut to_add: Vec<((VoiceId, String), (ParamRouteTarget, String))> = Vec::new();
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
        to_add.push((
            (r.source_voice, r.source_port.clone()),
            (r.target, r.target_param.clone()),
        ));
    }
    let map: &mut ParamRouteMap = match kind {
        ParamMapKind::Set => &mut state.param_routes_set,
        ParamMapKind::Bend => &mut state.param_routes_bend,
        ParamMapKind::Trigger => &mut state.param_routes_trigger,
    };
    for (src_key, target_pair) in to_add {
        let entry = map.entry(src_key).or_default();
        if !entry.iter().any(|t| *t == target_pair) {
            entry.push(target_pair);
        }
    }
}

/// Collect the control buses of every source whose target list contains
/// `tgt`, sorted by raw bus id for deterministic input ordering.
///
/// For ar-rate sources (entered via `.to_param_audio()`) the source's own
/// audio bus is unreadable by `In.kr`, so the runtime spawns a shared
/// `a2k_adapter_1` synth that downsamples to a kr bus; this helper swaps
/// in the adapter's bus when one is registered, keeping the summer
/// oblivious to the upstream rate.
fn collect_source_buses(
    state: &State,
    map: &ParamRouteMap,
    tgt: &(ParamRouteTarget, String),
) -> Vec<BusId> {
    let mut source_buses: Vec<BusId> = Vec::new();
    for ((sv, sp), targets) in map.iter() {
        if !targets.iter().any(|t| t == tgt) {
            continue;
        }
        let key = (*sv, sp.clone());
        if let Some((_, bus)) = state.ar_to_kr_adapters.get(&key) {
            source_buses.push(BusId::new(bus.raw()));
            continue;
        }
        if let Some(bus) = state.voices.get(sv).and_then(|v| {
            v.output_buses
                .iter()
                .find(|(n, _)| n == sp)
                .map(|(_, b)| *b)
        }) {
            source_buses.push(bus);
        }
    }
    source_buses.sort_by_key(|b| b.raw());
    source_buses
}

/// Look up the user-set baseline for `(target, target_param)` — the last
/// value passed to `voice.set_param(param, value)` or `fx.param(name,
/// value)`. `None` if the target isn't registered or the param has never
/// been set; the caller decides whether to fall back to the synthdef
/// default.
fn baseline_for_target(state: &State, tgt: &(ParamRouteTarget, String)) -> Option<f32> {
    match tgt.0 {
        ParamRouteTarget::Voice(vid) => state
            .voices
            .get(&vid)
            .and_then(|v| v.config.params.get(&tgt.1).copied()),
        ParamRouteTarget::Effect(eid) => state
            .effects
            .get(&eid)
            .and_then(|e| e.params.get(&tgt.1).copied()),
    }
}

impl<B: Backend> RoutesHandler<B> {
    /// Create a new routes handler.
    pub fn new(backend: Arc<B>, state: Arc<RwLock<State>>) -> Self {
        Self { backend, state }
    }

    /// Lazily create the param-summer group at the head of the root group.
    ///
    /// Summers (`param_kr_modulate_<n>` for BEND, future SET-side summers) live
    /// here so they tick before voice synths read their `/n_map`-bound params.
    /// Created on first call; subsequent calls return the cached node ID.
    async fn ensure_param_summer_group(&self) -> Result<NodeId> {
        let group_node = {
            let mut state = self.state.write().await;
            if let Some(node) = state.param_summer_group {
                return Ok(node);
            }
            let node_id = state.alloc_node_id();
            state.param_summer_group = Some(node_id);
            node_id
        };
        self.backend
            .create_group(group_node, NodeId::new(0), AddAction::Head)
            .await
            .map_err(Error::backend)?;
        tracing::debug!(
            "Created param-summer group {:?} at head of root",
            group_node
        );
        Ok(group_node)
    }

    /// Compute the additions and removals between two route maps at
    /// `(voice, port, dest)`-edge granularity.
    ///
    /// Each port can fan out to multiple destinations (Group fan-out;
    /// Main/Muted are always single). Adding a new group to an existing
    /// port surfaces as a single addition for the new edge; dropping one
    /// of N groups surfaces as a single removal. Re-pointing a port from
    /// `Main` → `Group(g)` is removal of the old + addition of the new.
    pub fn diff(old: &RouteMap, new: &RouteMap) -> RouteDiff {
        let mut diff = RouteDiff::default();

        for ((voice_id, port_name), new_dests) in new {
            let old_dests = old.get(&(*voice_id, port_name.clone()));
            for d in new_dests {
                let in_old = old_dests
                    .map(|v| v.iter().any(|x| x == d))
                    .unwrap_or(false);
                if !in_old {
                    diff.additions.push(Route {
                        voice_id: *voice_id,
                        port_name: port_name.clone(),
                        dest: d.clone(),
                    });
                }
            }
        }

        for ((voice_id, port_name), old_dests) in old {
            let new_dests = new.get(&(*voice_id, port_name.clone()));
            for d in old_dests {
                let in_new = new_dests
                    .map(|v| v.iter().any(|x| x == d))
                    .unwrap_or(false);
                if !in_new {
                    diff.removals.push(Route {
                        voice_id: *voice_id,
                        port_name: port_name.clone(),
                        dest: d.clone(),
                    });
                }
            }
        }

        diff
    }

    /// Reject route additions whose source port is kr-rate and whose
    /// destination is `RouteDest::Group(_)` — regardless of whether the
    /// group is hardware-routed.
    ///
    /// kr-rate ports feed control buses; group destinations always feed
    /// audio paths (`system_link_audio[_mono]` for hw-output groups,
    /// audio-bus mixing into the parent for sub-groups). Either path
    /// reads with `In.ar`, so kr data lands as DC bias / undefined output
    /// — silent at hw outputs, leaking garbage into the parent chain for
    /// non-hw sub-groups.
    ///
    /// Tr-rate ports are not yet covered — current scope is Kr only.
    /// kr-port → `RouteDest::Param`, `RouteDest::Main`, or
    /// `RouteDest::Muted` are all left to existing handling.
    fn validate_kr_to_group(state: &State, r: &Route) -> Result<()> {
        let group_id = match &r.dest {
            RouteDest::Group(g) => *g,
            _ => return Ok(()),
        };
        let voice = match state.voices.get(&r.voice_id) {
            Some(v) => v,
            None => return Ok(()),
        };
        let ports = state.synthdef_outputs(&voice.config.synthdef);
        let port_rate = match ports.iter().find(|p| p.name == r.port_name) {
            Some(p) => p.rate,
            None => return Ok(()),
        };
        if !matches!(port_rate, PortRate::Kr) {
            return Ok(());
        }
        let group = match state.groups.get(&group_id) {
            Some(g) => g,
            None => return Ok(()),
        };
        Err(Error::InvalidConfig(format!(
            "Voice '{voice_name}': port '{port}' (kr-rate) cannot route to group '{group_name}'. \
kr-rate ports carry control signals; group destinations feed audio paths and read with \
`In.ar`, producing DC bias / undefined output. Either:\n  \
  - declare the synthdef's port as `.output(...)` (ar) instead of `.output_kr(...)`,\n  \
  - or use the ar-rate variant of the source UGen (e.g. sin_osc_ar instead of sin_osc_kr),\n  \
  - or route to a param via `.to_param(target, \"param\")`, `.to_param_audio(target, \"param\")`, \
or `target.param(\"param\").modulate_by(source, \"port\")`.",
            voice_name = voice.config.name,
            port = r.port_name,
            group_name = group.name,
        )))
    }

    /// Apply a route diff: free old mixer synths, instantiate new ones.
    ///
    /// Order:
    /// 0. Validate every addition against the kr-port → hw-group rule.
    ///    If any addition violates, return Err before any spawn or free
    ///    runs — the route diff stays atomic, the script-side error is
    ///    surfaced unambiguously.
    /// 1. Free mixer synths for every removed `(voice, port, dest)` edge.
    /// 2. Instantiate mixer synths for every added edge.
    ///
    /// Step 1 runs before step 2 so a re-pointed route can reuse the same
    /// node-id pool slot; both steps drop the state lock before each backend
    /// call to preserve the project's lock discipline.
    pub async fn finalize(&self, diff: &RouteDiff) -> Result<()> {
        if diff.is_empty() {
            return Ok(());
        }

        {
            let state = self.state.read().await;
            for r in &diff.additions {
                Self::validate_kr_to_group(&state, r)?;
            }
        }

        let nodes_to_free: Vec<NodeId> = {
            let mut state = self.state.write().await;
            let mut nodes = Vec::new();
            for r in &diff.removals {
                let key = (r.voice_id, r.port_name.clone(), r.dest.clone());
                if let Some(node_id) = state.route_synths.remove(&key) {
                    state.free_node_id(node_id);
                    nodes.push(node_id);
                } else {
                    tracing::debug!(
                        "RoutesHandler::finalize: no live mixer for torn-down route voice={:?} port={:?} dest={:?} (already freed or never spawned, e.g. Muted)",
                        r.voice_id,
                        r.port_name,
                        r.dest,
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
                    "RoutesHandler::finalize: failed to spawn mixer for addition {:?}/{:?}/{:?}: {}",
                    r.voice_id,
                    r.port_name,
                    r.dest,
                    e
                );
            }
        }

        Ok(())
    }

    /// Reconcile named-input routes against [`State::input_routes`].
    ///
    /// This is the input-side counterpart to [`Self::finalize`]: it diffs the
    /// last materialized input-route map against the desired map, frees stale
    /// `input_link_*` nodes, then spawns one link node per newly added input
    /// edge. Target input buses are allocated lazily and kept stable on the
    /// owning [`crate::state::VoiceState`] for the voice lifetime.
    pub async fn finalize_input_routes(&self, desired: &InputRouteMap) -> Result<()> {
        let diff = {
            let state = self.state.read().await;
            compute_input_route_diff(&state.input_routes, desired)
        };
        if diff.is_empty() {
            return Ok(());
        }

        let nodes_to_free: Vec<NodeId> = {
            let mut state = self.state.write().await;
            let mut nodes = Vec::new();
            for r in &diff.removals {
                let key = (r.voice_id, r.port_name.clone(), r.src.clone());
                if let Some(node_id) = state.input_route_synths.remove(&key) {
                    state.free_node_id(node_id);
                    nodes.push(node_id);
                } else {
                    tracing::debug!(
                        "RoutesHandler::finalize_input_routes: no live input link for torn-down route voice={:?} port={:?} src={:?}",
                        r.voice_id,
                        r.port_name,
                        r.src,
                    );
                }
            }
            Self::sync_input_routes_to_live_synths(&mut state, desired);
            nodes
        };
        for node_id in nodes_to_free {
            tracing::debug!(
                "RoutesHandler::finalize_input_routes: freeing input route node {:?}",
                node_id
            );
            let _ = self.backend.free_node(node_id).await;
        }

        for r in &diff.additions {
            let spawn = match self.plan_input_route_spawn(r).await {
                Ok(spawn) => spawn,
                Err(e) => {
                    tracing::warn!(
                        "RoutesHandler::finalize_input_routes: failed to plan input link for addition {:?}/{:?}/{:?}: {}",
                        r.voice_id,
                        r.port_name,
                        r.src,
                        e
                    );
                    continue;
                }
            };

            let mut params = ParamMap::new();
            params.insert("in_bus".to_string(), spawn.in_bus.raw() as f32);
            params.insert("out_bus".to_string(), spawn.out_bus.raw() as f32);

            if let Err(e) = self
                .backend
                .create_synth(
                    spawn.synthdef,
                    spawn.node,
                    spawn.target_group,
                    AddAction::Tail,
                    &params,
                )
                .await
            {
                self.recycle_planned_input_route_node(spawn.node).await;
                return Err(Error::backend(e));
            }

            for node in spawn.target_nodes {
                if let Err(e) = self
                    .backend
                    .set_param(node, &spawn.target_param, spawn.out_bus.raw() as f32)
                    .await
                {
                    let _ = self.backend.free_node(spawn.node).await;
                    self.recycle_planned_input_route_node(spawn.node).await;
                    return Err(Error::backend(e));
                }
            }

            {
                let mut state = self.state.write().await;
                state
                    .input_route_synths
                    .insert((r.voice_id, r.port_name.clone(), r.src.clone()), spawn.node);
                Self::sync_input_routes_to_live_synths(&mut state, desired);
            }
        }

        Ok(())
    }

    fn sync_input_routes_to_live_synths(state: &mut State, desired: &InputRouteMap) {
        let live: HashSet<(VoiceId, String, InputRouteSrc)> =
            state.input_route_synths.keys().cloned().collect();
        let mut materialized = InputRouteMap::new();

        for ((voice_id, port_name), srcs) in desired {
            let live_srcs: Vec<InputRouteSrc> = srcs
                .iter()
                .filter(|src| live.contains(&(*voice_id, port_name.clone(), (*src).clone())))
                .cloned()
                .collect();
            if !live_srcs.is_empty() {
                materialized.insert((*voice_id, port_name.clone()), live_srcs);
            }
        }

        state.input_routes = materialized;
    }

    async fn recycle_planned_input_route_node(&self, node: NodeId) {
        let mut state = self.state.write().await;
        state.free_node_id(node);
    }

    async fn plan_input_route_spawn(&self, route: &InputRoute) -> Result<InputRouteSpawn> {
        let mut state = self.state.write().await;

        let (synthdef, target_group_id, target_nodes) = {
            let voice = state
                .voices
                .get(&route.voice_id)
                .ok_or(Error::VoiceNotFound(route.voice_id))?;
            let mut nodes = voice.active_nodes.clone();
            nodes.extend(voice.note_nodes.values().copied());
            (voice.config.synthdef.clone(), voice.config.group, nodes)
        };

        let inputs = state.synthdef_inputs(&synthdef);
        let (input_index, input_port) = inputs
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == route.port_name)
            .ok_or_else(|| {
                Error::SynthDefNotFound(format!(
                    "input port {:?} on voice {:?}",
                    route.port_name, route.voice_id
                ))
            })?;
        Self::validate_input_port_for_link(input_port)?;

        let in_bus = Self::resolve_input_route_source_bus(&mut state, input_port, &route.src)?;

        let target_group = state
            .groups
            .get(&target_group_id)
            .ok_or(Error::GroupNotFound(target_group_id))?
            .node_id;

        let out_bus = if let Some(bus) = state.voices.get(&route.voice_id).and_then(|voice| {
            voice
                .input_buses
                .iter()
                .find(|(name, _)| name == &route.port_name)
                .map(|(_, bus)| *bus)
        }) {
            bus
        } else {
            let bus = state.alloc_audio_bus(input_port.channels);
            state
                .voices
                .get_mut(&route.voice_id)
                .ok_or(Error::VoiceNotFound(route.voice_id))?
                .input_buses
                .push((route.port_name.clone(), bus));
            bus
        };

        let node = state.alloc_node_id();

        let synthdef = match input_port.channels {
            1 => "input_link_1",
            2 => "input_link_2",
            n => {
                return Err(Error::SynthDefNotFound(format!(
                    "no input_link_<channels> built-in for {} channels",
                    n
                )));
            }
        };

        Ok(InputRouteSpawn {
            synthdef,
            node,
            target_group,
            in_bus,
            out_bus,
            target_param: vibelang_dsp::builder::input_bus_param_name(input_index),
            target_nodes,
        })
    }

    fn validate_input_port_for_link(port: &InputPort) -> Result<()> {
        if port.rate != PortRate::Ar {
            return Err(Error::InvalidConfig(format!(
                "Input port '{}' is {:?}-rate; input_link_* currently supports audio-rate inputs",
                port.name, port.rate
            )));
        }
        if !matches!(port.channels, 1 | 2) {
            return Err(Error::SynthDefNotFound(format!(
                "no input_link_<channels> built-in for {} channels",
                port.channels
            )));
        }
        Ok(())
    }

    fn resolve_input_route_source_bus(
        state: &mut State,
        target_port: &InputPort,
        src: &InputRouteSrc,
    ) -> Result<BusId> {
        match src {
            InputRouteSrc::Voice(source_voice, output_port) => {
                let voice = state
                    .voices
                    .get(source_voice)
                    .ok_or(Error::VoiceNotFound(*source_voice))?;
                let bus = voice
                    .output_buses
                    .iter()
                    .find(|(name, _)| name == output_port)
                    .map(|(_, bus)| *bus)
                    .ok_or_else(|| {
                        Error::SynthDefNotFound(format!(
                            "output port {:?} on voice {:?}",
                            output_port, source_voice
                        ))
                    })?;
                let ports = state.synthdef_outputs(&voice.config.synthdef);
                let source_port = ports
                    .iter()
                    .find(|p| p.name == *output_port)
                    .ok_or_else(|| {
                        Error::SynthDefNotFound(format!(
                            "output port {:?} on voice {:?}",
                            output_port, source_voice
                        ))
                    })?;
                Self::validate_input_source_width(target_port, source_port.channels)?;
                if source_port.rate != PortRate::Ar {
                    return Err(Error::InvalidConfig(format!(
                        "Output port '{}' is {:?}-rate; named audio inputs require an audio-rate source",
                        output_port, source_port.rate
                    )));
                }
                Ok(bus)
            }
            InputRouteSrc::Group(group_id) => {
                Self::validate_input_source_width(target_port, 2)?;
                state
                    .groups
                    .get(group_id)
                    .map(|g| g.audio_bus)
                    .ok_or(Error::GroupNotFound(*group_id))
            }
            InputRouteSrc::HardwareInput(channels) => {
                if channels.len() != usize::from(target_port.channels) {
                    return Err(Error::InvalidConfig(format!(
                        "Hardware input route for '{}' has {} channel(s), expected {}",
                        target_port.name,
                        channels.len(),
                        target_port.channels
                    )));
                }
                let first = channels.first().copied().ok_or_else(|| {
                    Error::InvalidConfig(format!(
                        "Hardware input route for '{}' must name at least one channel",
                        target_port.name
                    ))
                })?;
                for (offset, channel) in channels.iter().enumerate() {
                    if *channel != first + offset as u32 {
                        return Err(Error::InvalidConfig(format!(
                            "Hardware input route for '{}' must use contiguous channels; got {:?}",
                            target_port.name, channels
                        )));
                    }
                }
                Ok(BusId::new(state.hardware_input_offset + first))
            }
            InputRouteSrc::Silent => {
                if target_port.channels > 2 {
                    return Err(Error::InvalidConfig(format!(
                        "Silent input source supports up to stereo, but input '{}' needs {} channels",
                        target_port.name, target_port.channels
                    )));
                }
                let bus = match state.silent_ar_bus {
                    Some(bus) => bus,
                    None => {
                        let bus = state.alloc_audio_bus(2);
                        state.silent_ar_bus = Some(bus);
                        bus
                    }
                };
                Ok(bus)
            }
        }
    }

    fn validate_input_source_width(target_port: &InputPort, source_channels: u8) -> Result<()> {
        if source_channels != target_port.channels {
            return Err(Error::InvalidConfig(format!(
                "Input port '{}' expects {} channel(s), source provides {}",
                target_port.name, target_port.channels, source_channels
            )));
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
            for (tgt, tgt_param) in new_targets {
                let in_old = old_targets
                    .map(|v| v.iter().any(|t| t.0 == *tgt && t.1 == *tgt_param))
                    .unwrap_or(false);
                if !in_old {
                    diff.additions.push(ParamRoute {
                        source_voice: *src_voice,
                        source_port: src_port.clone(),
                        target: *tgt,
                        target_param: tgt_param.clone(),
                    });
                }
            }
        }

        for ((src_voice, src_port), old_targets) in old {
            let new_targets = new.get(&(*src_voice, src_port.clone()));
            for (tgt, tgt_param) in old_targets {
                let in_new = new_targets
                    .map(|v| v.iter().any(|t| t.0 == *tgt && t.1 == *tgt_param))
                    .unwrap_or(false);
                if !in_new {
                    diff.removals.push(ParamRoute {
                        source_voice: *src_voice,
                        source_port: src_port.clone(),
                        target: *tgt,
                        target_param: tgt_param.clone(),
                    });
                }
            }
        }

        diff
    }

    /// Apply SET + BEND Param-route diffs in one pass.
    ///
    /// Multi-output v3 unified routing: both `.to_param` (SET) and
    /// `.modulate_by` (BEND) spawn a `param_kr_modulate_<n>` summer that
    /// reads `baseline + Σ (scale_i * In.kr(in_i, 1) + offset_i)` and
    /// writes the result to an intermediate control bus; the target is
    /// `/n_map`-bound to that bus. The verb difference is purely the
    /// `baseline` source:
    ///
    /// - **SET path** (`.to_param`): `baseline=0`. The source signal flows
    ///   through unchanged at the default `scale=1, offset=0`; the user's
    ///   `set_param` value is silently masked while the route is active.
    ///   Multi-source on the same target is rejected at script time;
    ///   runtime drops the late entries with a warning if it sees them.
    /// - **BEND path** (`.modulate_by`): `baseline` rides the user's last
    ///   `set_param` value so modulators add on top. `voices::set_param`
    ///   pokes the summer's `baseline` control on subsequent updates.
    ///   Respawns when the source set's arity changes.
    ///
    /// Order:
    /// 1. Stage both diffs against [`State::param_routes_set`] /
    ///    [`State::param_routes_bend`] under one lock; collect per-target
    ///    actions and the list of summer nodes/buses to free.
    /// 2. Free torn-down summers on the backend first so reused node IDs
    ///    don't collide with newly spawned ones.
    /// 3. Drive each planned action. Targets are deduplicated across SET +
    ///    BEND; an addition that *moves* a target between maps emits an
    ///    unmap of the old kind followed by the new kind.
    ///
    /// All backend calls are issued outside the state lock.
    pub async fn finalize_params(
        &self,
        set_diff: &ParamRouteDiff,
        bend_diff: &ParamRouteDiff,
        trigger_diff: &ParamRouteDiff,
    ) -> Result<()> {
        if set_diff.is_empty() && bend_diff.is_empty() && trigger_diff.is_empty() {
            return Ok(());
        }

        // Summers, trigger links, and ar→kr adapters all run in a dedicated
        // group at the head of the root group so they tick before voice
        // synths read their `/n_map`-bound params.
        if !set_diff.additions.is_empty()
            || !bend_diff.additions.is_empty()
            || !trigger_diff.additions.is_empty()
        {
            self.ensure_param_summer_group().await?;
        }

        let (
            planned,
            summers_to_free,
            adapters_to_spawn,
            adapters_to_free,
            triggers_to_free,
        ) = self
            .plan_param_actions(set_diff, bend_diff, trigger_diff)
            .await;

        for &(node, _) in &summers_to_free {
            if let Err(e) = self.backend.free_node(node).await {
                tracing::warn!(
                    "RoutesHandler::finalize_params: failed to free old summer node {:?}: {}",
                    node,
                    e,
                );
            }
        }
        for &(node, _) in &triggers_to_free {
            if let Err(e) = self.backend.free_node(node).await {
                tracing::warn!(
                    "RoutesHandler::finalize_params: failed to free old trigger link node {:?}: {}",
                    node,
                    e,
                );
            }
        }
        for &(node, _) in &adapters_to_free {
            if let Err(e) = self.backend.free_node(node).await {
                tracing::warn!(
                    "RoutesHandler::finalize_params: failed to free old a2k adapter node {:?}: {}",
                    node,
                    e,
                );
            }
        }
        if !summers_to_free.is_empty()
            || !triggers_to_free.is_empty()
            || !adapters_to_free.is_empty()
        {
            let mut state = self.state.write().await;
            for (node, bus) in summers_to_free {
                state.free_node_id(node);
                state.free_control_bus(bus);
            }
            for (node, bus) in triggers_to_free {
                state.free_node_id(node);
                state.free_control_bus(bus);
            }
            for (node, bus) in adapters_to_free {
                state.free_node_id(node);
                state.free_control_bus(bus);
            }
        }

        // Spawn ar→kr adapters before any new summer so the summer's first
        // tick can read a populated kr bus rather than a freshly-allocated
        // (and therefore zeroed) one. Adapters live at the Head of the
        // param-summer group; summers attach at the Tail. Both block on
        // ensure_param_summer_group so the group node is always live by
        // the time a synth targets it.
        if !adapters_to_spawn.is_empty() {
            let summer_group = self.ensure_param_summer_group().await?;
            for spawn in adapters_to_spawn {
                let mut params = ParamMap::new();
                params.insert("in_bus".to_string(), spawn.in_bus.raw() as f32);
                params.insert("out_bus".to_string(), spawn.out_bus.raw() as f32);
                self.backend
                    .create_synth(
                        "a2k_adapter_1",
                        spawn.node,
                        summer_group,
                        AddAction::Head,
                        &params,
                    )
                    .await
                    .map_err(Error::backend)?;
            }
        }

        for action in planned {
            self.apply_param_action(action).await?;
        }

        Ok(())
    }

    /// Stage `state.param_routes_set` / `state.param_routes_bend` /
    /// `state.param_routes_trigger` and `state.param_summers` /
    /// `state.param_triggers` against the diffs, returning per-target actions
    /// plus any summer / trigger nodes/buses to free on the backend
    /// afterwards, plus the ar→kr adapters that need to be spawned (for
    /// newly-introduced ar sources) or freed (for sources whose last param
    /// route was removed).
    async fn plan_param_actions(
        &self,
        set_diff: &ParamRouteDiff,
        bend_diff: &ParamRouteDiff,
        trigger_diff: &ParamRouteDiff,
    ) -> (
        Vec<PlannedParamAction>,
        Vec<(NodeId, ControlBusId)>,
        Vec<AdapterSpawn>,
        Vec<(NodeId, ControlBusId)>,
        Vec<(NodeId, ControlBusId)>,
    ) {
        let mut state = self.state.write().await;

        apply_param_diff_to_map(&mut state, set_diff, ParamMapKind::Set);
        apply_param_diff_to_map(&mut state, bend_diff, ParamMapKind::Bend);
        apply_param_diff_to_map(&mut state, trigger_diff, ParamMapKind::Trigger);

        // Reconcile ar→kr adapters with the post-diff source set. We do
        // this *before* gathering source buses for summers so
        // `collect_source_buses` sees the freshly-registered adapter buses.
        // Adapter spawn is one-per-(source_voice, source_port), shared by
        // all routes (SET / BEND / TRIGGER) that originate from that pair —
        // trigger sources are always Tr-rate so the adapter filter rejects
        // them, but they're included so the cleanup pass tears down a
        // stale adapter when a port flips from ar to tr.
        let active_sources: HashSet<(VoiceId, String)> = state
            .param_routes_set
            .keys()
            .chain(state.param_routes_bend.keys())
            .chain(state.param_routes_trigger.keys())
            .cloned()
            .collect();

        let mut adapters_to_spawn: Vec<AdapterSpawn> = Vec::new();
        let need_adapter: Vec<(VoiceId, String, BusId)> = active_sources
            .iter()
            .filter(|key| !state.ar_to_kr_adapters.contains_key(key))
            .filter_map(|(sv, sp)| {
                let voice = state.voices.get(sv)?;
                let synth = voice.config.synthdef.clone();
                let port_rate = state
                    .synthdef_outputs(&synth)
                    .into_iter()
                    .find(|p| p.name == *sp)
                    .map(|p| p.rate);
                if port_rate != Some(PortRate::Ar) {
                    return None;
                }
                let in_bus = voice
                    .output_buses
                    .iter()
                    .find(|(n, _)| n == sp)
                    .map(|(_, b)| *b)?;
                Some((*sv, sp.clone(), in_bus))
            })
            .collect();
        for (sv, sp, in_bus) in need_adapter {
            let out_bus = state.alloc_control_bus();
            let node = state.alloc_node_id();
            state
                .ar_to_kr_adapters
                .insert((sv, sp), (node, out_bus));
            adapters_to_spawn.push(AdapterSpawn {
                node,
                in_bus,
                out_bus,
            });
        }

        let mut adapters_to_free: Vec<(NodeId, ControlBusId)> = Vec::new();
        let stale_adapter_keys: Vec<(VoiceId, String)> = state
            .ar_to_kr_adapters
            .keys()
            .filter(|k| !active_sources.contains(k))
            .cloned()
            .collect();
        for k in stale_adapter_keys {
            if let Some((node, bus)) = state.ar_to_kr_adapters.remove(&k) {
                adapters_to_free.push((node, bus));
            }
        }

        // Build a deterministic order of affected target keys: removals
        // first, then additions; deduplicated while preserving first-seen
        // order. All three diffs contribute.
        let mut seen: HashSet<(ParamRouteTarget, String)> = HashSet::new();
        let mut affected: Vec<(ParamRouteTarget, String)> = Vec::new();
        for r in set_diff
            .removals
            .iter()
            .chain(bend_diff.removals.iter())
            .chain(trigger_diff.removals.iter())
            .chain(set_diff.additions.iter())
            .chain(bend_diff.additions.iter())
            .chain(trigger_diff.additions.iter())
        {
            let key = (r.target, r.target_param.clone());
            if seen.insert(key.clone()) {
                affected.push(key);
            }
        }

        let mut planned = Vec::with_capacity(affected.len());
        let mut summers_to_free: Vec<(NodeId, ControlBusId)> = Vec::new();
        let mut triggers_to_free: Vec<(NodeId, ControlBusId)> = Vec::new();

        for tgt in affected {
            // Tear down any existing summer or trigger link for this
            // target up-front. Respawning the summer whenever the
            // (set ∪ bend) source set is non-empty ensures the summer's
            // parameter list reflects the new source bus IDs and per-source
            // scale/offset values; respawning the trigger link rebinds the
            // forwarder when the source bus changes. Both maps are torn
            // down here so a transition between verb kinds (e.g. SET → TRIGGER)
            // frees the old kind's resources before the new one spawns.
            if let Some(prev) = state.param_summers.remove(&tgt) {
                summers_to_free
                    .push((prev.node, ControlBusId::new(prev.bus.raw())));
            }
            if let Some((prev_node, prev_bus)) = state.param_triggers.remove(&tgt) {
                triggers_to_free.push((prev_node, ControlBusId::new(prev_bus.raw())));
            }

            // Voice targets contribute every active node + every note node;
            // fx targets contribute their single effect node.
            let target_nodes: Vec<NodeId> = match tgt.0 {
                ParamRouteTarget::Voice(vid) => state
                    .voices
                    .get(&vid)
                    .map(|v| {
                        v.active_nodes
                            .iter()
                            .copied()
                            .chain(v.note_nodes.values().copied())
                            .collect()
                    })
                    .unwrap_or_default(),
                ParamRouteTarget::Effect(eid) => state
                    .effects
                    .get(&eid)
                    .map(|e| vec![e.node_id])
                    .unwrap_or_default(),
            };

            // Gather SET / BEND / TRIGGER sources targeting this `(target,
            // param)`.
            let set_buses = collect_source_buses(&state, &state.param_routes_set, &tgt);
            let bend_buses = collect_source_buses(&state, &state.param_routes_bend, &tgt);
            let trigger_buses =
                collect_source_buses(&state, &state.param_routes_trigger, &tgt);

            let summer_active = !set_buses.is_empty() || !bend_buses.is_empty();
            let trigger_active = !trigger_buses.is_empty();

            let plan = if summer_active && trigger_active {
                tracing::warn!(
                    "RoutesHandler::finalize_params: target {:?} param {:?} has \
                     both summer (SET/BEND) and TRIGGER sources — treating as \
                     empty (cross-verb conflict should have been caught at \
                     script time)",
                    tgt.0,
                    tgt.1,
                );
                let restore_value = baseline_for_target(&state, &tgt);
                ParamPlan::Unmap { restore_value }
            } else if trigger_active {
                if trigger_buses.len() > 1 {
                    tracing::warn!(
                        "RoutesHandler::finalize_params: target {:?} param {:?} has \
                         {} TRIGGER sources — multi-source TRIGGER is disallowed; \
                         using only the first (script-time validation should have \
                         rejected this)",
                        tgt.0,
                        tgt.1,
                        trigger_buses.len(),
                    );
                }
                self.plan_trigger_link(&mut state, &tgt, trigger_buses[0])
            } else if !set_buses.is_empty() && !bend_buses.is_empty() {
                tracing::warn!(
                    "RoutesHandler::finalize_params: target {:?} param {:?} has both \
                     SET and BEND sources — treating as empty (cross-verb conflict \
                     should have been caught at script time)",
                    tgt.0,
                    tgt.1,
                );
                let restore_value = baseline_for_target(&state, &tgt);
                ParamPlan::Unmap { restore_value }
            } else if !set_buses.is_empty() {
                // SET path through the unified summer: pin baseline=0 so
                // the source signal flows through unchanged at the default
                // scale=1/offset=0 shaping (the SET "replace" semantic).
                if set_buses.len() > 1 {
                    tracing::warn!(
                        "RoutesHandler::finalize_params: target {:?} param {:?} has \
                         {} SET sources — multi-source SET is disallowed; using only \
                         the first (script-time validation should have rejected this)",
                        tgt.0,
                        tgt.1,
                        set_buses.len(),
                    );
                }
                let used = vec![set_buses[0]];
                self.plan_summer(&mut state, &tgt, &used, /*baseline=*/ 0.0)
            } else if !bend_buses.is_empty() {
                // BEND path: baseline rides the user's set_param value so
                // modulators add on top.
                let max_n = vibelang_dsp::system_synthdefs::PARAM_KR_MODULATE_MAX;
                let used: Vec<BusId> = if bend_buses.len() > max_n {
                    tracing::warn!(
                        "RoutesHandler::finalize_params: {} bend sources point at \
                         target {:?} param {:?}, exceeds max {}; truncating",
                        bend_buses.len(),
                        tgt.0,
                        tgt.1,
                        max_n,
                    );
                    bend_buses.into_iter().take(max_n).collect()
                } else {
                    bend_buses
                };
                let baseline = baseline_for_target(&state, &tgt).unwrap_or(0.0);
                self.plan_summer(&mut state, &tgt, &used, baseline)
            } else {
                let restore_value = baseline_for_target(&state, &tgt);
                ParamPlan::Unmap { restore_value }
            };

            planned.push(PlannedParamAction {
                target: tgt.0,
                target_param: tgt.1,
                target_nodes,
                plan,
            });
        }

        (
            planned,
            summers_to_free,
            adapters_to_spawn,
            adapters_to_free,
            triggers_to_free,
        )
    }

    /// Allocate intermediate bus + summer node, register
    /// [`ParamSummerState`] with default per-source scale=1.0 / offset=0.0,
    /// and produce a [`ParamPlan::Summer`] action.
    ///
    /// Default scale/offset are wired to the summer params on first spawn
    /// so subsequent `.scale()` / `.offset()` builder updates can be poked
    /// via `/n_set` against the recorded summer node without a respawn.
    fn plan_summer(
        &self,
        state: &mut State,
        tgt: &(ParamRouteTarget, String),
        used: &[BusId],
        baseline: f32,
    ) -> ParamPlan {
        let arity = used.len();
        let intermediate = state.alloc_control_bus();
        let intermediate_bus = BusId::new(intermediate.raw());
        let summer_node = state.alloc_node_id();
        let sources: Vec<ParamSummerSource> = used
            .iter()
            .map(|b| ParamSummerSource {
                bus: *b,
                scale: 1.0,
                offset: 0.0,
            })
            .collect();
        state.param_summers.insert(
            tgt.clone(),
            ParamSummerState {
                node: summer_node,
                bus: intermediate_bus,
                sources: sources.clone(),
            },
        );

        let target_group = state
            .param_summer_group
            .unwrap_or_else(|| NodeId::new(0));

        let mut params = ParamMap::new();
        params.insert("baseline".to_string(), baseline);
        let port_letters = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
        for (i, src) in sources.iter().enumerate() {
            let letter = port_letters[i];
            params.insert(format!("in_{}", letter), src.bus.raw() as f32);
            params.insert(format!("scale_{}", letter), src.scale);
            params.insert(format!("offset_{}", letter), src.offset);
        }
        params.insert("out_bus".to_string(), intermediate_bus.raw() as f32);

        ParamPlan::Summer {
            synthdef: format!("param_kr_modulate_{}", arity),
            summer_node,
            target_group,
            params,
            intermediate_bus,
        }
    }

    /// Allocate intermediate bus + link node for a `port_tr_to_param_link_1`
    /// trigger forwarder, register the entry in [`State::param_triggers`],
    /// and produce a [`ParamPlan::TriggerLink`] action.
    ///
    /// Single-source: trigger routing is 1:1, no scale/offset shaping.
    fn plan_trigger_link(
        &self,
        state: &mut State,
        tgt: &(ParamRouteTarget, String),
        in_bus: BusId,
    ) -> ParamPlan {
        let intermediate = state.alloc_control_bus();
        let intermediate_bus = BusId::new(intermediate.raw());
        let link_node = state.alloc_node_id();
        state
            .param_triggers
            .insert(tgt.clone(), (link_node, intermediate_bus));

        let target_group = state
            .param_summer_group
            .unwrap_or_else(|| NodeId::new(0));

        let mut params = ParamMap::new();
        params.insert("in_bus".to_string(), in_bus.raw() as f32);
        params.insert("out_bus".to_string(), intermediate_bus.raw() as f32);

        ParamPlan::TriggerLink {
            link_node,
            target_group,
            params,
            intermediate_bus,
        }
    }

    /// Drive a single [`PlannedParamAction`] to the backend.
    async fn apply_param_action(&self, action: PlannedParamAction) -> Result<()> {
        match action.plan {
            ParamPlan::Unmap { restore_value } => {
                for node in &action.target_nodes {
                    if let Err(e) = self
                        .backend
                        .map_param_to_bus(*node, &action.target_param, u32::MAX)
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
                if let Some(value) = restore_value {
                    for node in action.target_nodes {
                        if let Err(e) = self
                            .backend
                            .set_param(node, &action.target_param, value)
                            .await
                        {
                            tracing::warn!(
                                "RoutesHandler::finalize_params: restore set_param failed node={:?} param={:?} value={}: {}",
                                node,
                                action.target_param,
                                value,
                                e,
                            );
                        }
                    }
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
            ParamPlan::TriggerLink {
                link_node,
                target_group,
                params,
                intermediate_bus,
            } => {
                self.backend
                    .create_synth(
                        "port_tr_to_param_link_1",
                        link_node,
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
                .insert((voice_id, port_name.to_string(), dest.clone()), link_node);

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
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
        assert_eq!(routes[0].1, vec![RouteDest::Group(group)]);
    }

    #[test]
    fn default_routes_one_stereo_port_routes_straight_into_group() {
        // 1 stereo port → straight L/R into group (today's legacy behaviour).
        let group = GroupId::new(11);
        let routes = default_routes_for_voice(group, &[port("out", 2)]);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].0, "out");
        assert_eq!(routes[0].1, vec![RouteDest::Group(group)]);
    }

    #[test]
    fn default_routes_two_mono_ports_both_route_into_group() {
        // 2 mono ports → port[0]=L, port[1]=R into the group bus, summed at
        // the destination (dual-mono summed). Helper emits one route per port.
        let group = GroupId::new(3);
        let routes = default_routes_for_voice(group, &[port("L", 1), port("R", 1)]);
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].0, "L");
        assert_eq!(routes[0].1, vec![RouteDest::Group(group)]);
        assert_eq!(routes[1].0, "R");
        assert_eq!(routes[1].1, vec![RouteDest::Group(group)]);
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
            vec![RouteDest::Group(group_default)],
        );

        let mut user = RouteMap::new();
        user.insert(
            (voice_id, "out".to_string()),
            vec![RouteDest::Group(group_user)],
        );

        let merged = merge_default_routes(&user, &defaults);

        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[&(voice_id, "out".to_string())],
            vec![RouteDest::Group(group_user)],
            "user route must win over default"
        );
    }

    #[test]
    fn merge_default_routes_default_fills_unrouted_port() {
        let voice_id = VoiceId::new(5);
        let group = GroupId::new(8);

        let mut defaults = RouteMap::new();
        defaults.insert(
            (voice_id, "L".to_string()),
            vec![RouteDest::Group(group)],
        );
        defaults.insert(
            (voice_id, "R".to_string()),
            vec![RouteDest::Group(group)],
        );

        let mut user = RouteMap::new();
        user.insert((voice_id, "L".to_string()), vec![RouteDest::Main]);

        let merged = merge_default_routes(&user, &defaults);

        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged[&(voice_id, "L".to_string())],
            vec![RouteDest::Main]
        );
        assert_eq!(
            merged[&(voice_id, "R".to_string())],
            vec![RouteDest::Group(group)],
            "port without explicit route falls back to default"
        );
    }

    #[test]
    fn merge_default_routes_empty_user_returns_defaults() {
        let voice_id = VoiceId::new(1);
        let group = GroupId::new(1);
        let mut defaults = RouteMap::new();
        defaults.insert(
            (voice_id, "out".to_string()),
            vec![RouteDest::Group(group)],
        );

        let merged = merge_default_routes(&RouteMap::new(), &defaults);
        assert_eq!(merged, defaults);
    }

    // =========================================================================
    // Diff tests
    // =========================================================================

    fn make_map(entries: &[((u32, &str), RouteDest)]) -> RouteMap {
        entries
            .iter()
            .map(|((vid, port), dest)| {
                ((VoiceId::new(*vid), (*port).to_string()), vec![dest.clone()])
            })
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
    }

    #[test]
    fn diff_repointed_group_dest_is_remove_plus_add() {
        // Per-edge diff: re-pointing a port from group 1 to group 2 is a
        // removal of the old edge and an addition of the new edge.
        let old = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(1)))]);
        let new = make_map(&[((1, "out"), RouteDest::Group(GroupId::new(2)))]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert_eq!(diff.additions.len(), 1);
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(
            diff.removals[0].dest,
            RouteDest::Group(GroupId::new(1))
        );
        assert_eq!(
            diff.additions[0].dest,
            RouteDest::Group(GroupId::new(2))
        );
    }

    #[test]
    fn diff_repointed_main_to_muted_is_remove_plus_add() {
        let old = make_map(&[((3, "fx_send"), RouteDest::Main)]);
        let new = make_map(&[((3, "fx_send"), RouteDest::Muted)]);

        let diff = RoutesHandler::<MockBackend>::diff(&old, &new);

        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0].dest, RouteDest::Main);
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(diff.additions[0].dest, RouteDest::Muted);
    }

    // =========================================================================
    // Input-route diff tests (P3.2)
    // =========================================================================

    fn make_input_map(
        entries: &[((u32, &str), Vec<InputRouteSrc>)],
    ) -> InputRouteMap {
        entries
            .iter()
            .map(|((vid, port), srcs)| {
                ((VoiceId::new(*vid), (*port).to_string()), srcs.clone())
            })
            .collect()
    }

    #[test]
    fn input_diff_add_only() {
        let old = InputRouteMap::new();
        let new = make_input_map(&[(
            (10, "in"),
            vec![InputRouteSrc::Voice(VoiceId::new(2), "out".to_string())],
        )]);

        let diff = compute_input_route_diff(&old, &new);

        assert_eq!(diff.additions.len(), 1);
        assert!(diff.removals.is_empty());
        assert_eq!(diff.additions[0].voice_id, VoiceId::new(10));
        assert_eq!(diff.additions[0].port_name, "in");
        assert_eq!(
            diff.additions[0].src,
            InputRouteSrc::Voice(VoiceId::new(2), "out".to_string())
        );
    }

    #[test]
    fn input_diff_remove_only() {
        let old = make_input_map(&[((10, "in"), vec![InputRouteSrc::Group(GroupId::new(5))])]);
        let new = InputRouteMap::new();

        let diff = compute_input_route_diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0].src, InputRouteSrc::Group(GroupId::new(5)));
    }

    #[test]
    fn input_diff_replace_is_remove_plus_add() {
        let old = make_input_map(&[(
            (10, "in"),
            vec![InputRouteSrc::Voice(VoiceId::new(2), "out".to_string())],
        )]);
        let new = make_input_map(&[((10, "in"), vec![InputRouteSrc::Group(GroupId::new(5))])]);

        let diff = compute_input_route_diff(&old, &new);

        assert_eq!(diff.removals.len(), 1);
        assert_eq!(
            diff.removals[0].src,
            InputRouteSrc::Voice(VoiceId::new(2), "out".to_string())
        );
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(
            diff.additions[0].src,
            InputRouteSrc::Group(GroupId::new(5))
        );
    }

    #[test]
    fn input_diff_fan_in_grow_emits_one_addition() {
        let a = InputRouteSrc::Voice(VoiceId::new(2), "out".to_string());
        let b = InputRouteSrc::Group(GroupId::new(5));
        let old = make_input_map(&[((10, "in"), vec![a.clone()])]);
        let new = make_input_map(&[((10, "in"), vec![a.clone(), b.clone()])]);

        let diff = compute_input_route_diff(&old, &new);

        assert!(diff.removals.is_empty());
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(diff.additions[0].src, b);
    }

    #[test]
    fn input_diff_fan_in_shrink_emits_one_removal() {
        let a = InputRouteSrc::Voice(VoiceId::new(2), "out".to_string());
        let b = InputRouteSrc::Group(GroupId::new(5));
        let old = make_input_map(&[((10, "in"), vec![a.clone(), b.clone()])]);
        let new = make_input_map(&[((10, "in"), vec![a.clone()])]);

        let diff = compute_input_route_diff(&old, &new);

        assert!(diff.additions.is_empty());
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0].src, b);
    }

    #[test]
    fn input_diff_identical_is_empty() {
        let map = make_input_map(&[
            (
                (10, "in"),
                vec![InputRouteSrc::Voice(VoiceId::new(2), "out".to_string())],
            ),
            ((11, "side"), vec![InputRouteSrc::HardwareInput(vec![1, 2])]),
            ((12, "fb"), vec![InputRouteSrc::Silent]),
        ]);

        let diff = compute_input_route_diff(&map, &map);
        assert!(diff.is_empty());
    }

    #[test]
    fn input_diff_hardware_channel_set_change_is_remove_plus_add() {
        let old = make_input_map(&[((10, "in"), vec![InputRouteSrc::HardwareInput(vec![1, 2])])]);
        let new = make_input_map(&[((10, "in"), vec![InputRouteSrc::HardwareInput(vec![3, 4])])]);

        let diff = compute_input_route_diff(&old, &new);

        assert_eq!(diff.removals.len(), 1);
        assert_eq!(
            diff.removals[0].src,
            InputRouteSrc::HardwareInput(vec![1, 2])
        );
        assert_eq!(diff.additions.len(), 1);
        assert_eq!(
            diff.additions[0].src,
            InputRouteSrc::HardwareInput(vec![3, 4])
        );
    }

    #[test]
    fn input_diff_emits_in_deterministic_order() {
        let old = InputRouteMap::new();
        let new = make_input_map(&[
            ((20, "in"), vec![InputRouteSrc::Group(GroupId::new(2))]),
            (
                (10, "side"),
                vec![InputRouteSrc::Silent, InputRouteSrc::HardwareInput(vec![3])],
            ),
            (
                (10, "in"),
                vec![
                    InputRouteSrc::Group(GroupId::new(7)),
                    InputRouteSrc::Voice(VoiceId::new(3), "out".to_string()),
                ],
            ),
        ]);

        let first = compute_input_route_diff(&old, &new);
        let second = compute_input_route_diff(&old, &new);
        assert_eq!(first, second);

        let order: Vec<(u32, &str)> = first
            .additions
            .iter()
            .map(|r| (r.voice_id.raw(), r.port_name.as_str()))
            .collect();
        assert_eq!(
            order,
            vec![
                (10, "in"),
                (10, "in"),
                (10, "side"),
                (10, "side"),
                (20, "in"),
            ]
        );
        assert_eq!(
            first.additions[0].src,
            InputRouteSrc::Voice(VoiceId::new(3), "out".to_string())
        );
        assert_eq!(
            first.additions[1].src,
            InputRouteSrc::Group(GroupId::new(7))
        );
        assert_eq!(
            first.additions[2].src,
            InputRouteSrc::HardwareInput(vec![3])
        );
        assert_eq!(first.additions[3].src, InputRouteSrc::Silent);
        assert_eq!(
            first.additions[4].src,
            InputRouteSrc::Group(GroupId::new(2))
        );
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
        last_sets: Mutex<Vec<(NodeId, String, f32)>>,
        fail_next_create: AtomicBool,
        fail_next_set: AtomicBool,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                synths_created: AtomicU32::new(0),
                nodes_freed: AtomicU32::new(0),
                last_creates: Mutex::new(Vec::new()),
                last_frees: Mutex::new(Vec::new()),
                last_maps: Mutex::new(Vec::new()),
                last_sets: Mutex::new(Vec::new()),
                fail_next_create: AtomicBool::new(false),
                fail_next_set: AtomicBool::new(false),
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
        fn sets(&self) -> Vec<(NodeId, String, f32)> {
            self.last_sets.lock().unwrap().clone()
        }
        fn fail_next_create(&self) {
            self.fail_next_create.store(true, Ordering::Relaxed);
        }
        fn fail_next_set(&self) {
            self.fail_next_set.store(true, Ordering::Relaxed);
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
            if self.fail_next_create.swap(false, Ordering::Relaxed) {
                return Err(MockError);
            }
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
            node: NodeId,
            param: &str,
            value: f32,
        ) -> std::result::Result<(), Self::Error> {
            if self.fail_next_set.swap(false, Ordering::Relaxed) {
                return Err(MockError);
            }
            self.last_sets
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
                    output_channels: None,
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
                    output_channels: None,
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
                    input_buses: Vec::new(),
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
            .contains_key(&(voice_id, port_name, RouteDest::Group(dest_group_id))));
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
            .contains_key(&(voice_id, port_name, RouteDest::Muted)));
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
                    output_channels: None,
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
            .get(&(voice_id, port_name.clone(), RouteDest::Group(dest_a)))
            .copied()
            .unwrap();

        // Now move: re-point from A → B as a per-edge remove + add.
        let mut change_diff = RouteDiff::default();
        change_diff.removals.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_a),
        });
        change_diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_b),
        });
        handler.finalize(&change_diff).await.unwrap();

        // Old node freed, new node created.
        assert!(backend.frees().contains(&node_a), "old mixer was freed");
        let creates = backend.creates();
        assert_eq!(creates.len(), 2, "one create on add, one on re-point");
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
            .get(&(voice_id, port_name, RouteDest::Group(dest_b)))
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
            .contains_key(&(voice_id, port_name, RouteDest::Group(dest))));
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

    async fn setup_named_input_fixture(
        input_channels: u8,
        source_channels: u8,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        VoiceId,
        VoiceId,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let target_id = VoiceId::new(10);
        let source_id = VoiceId::new(20);
        let group_id = GroupId::new(1);

        {
            let mut s = state.write().await;
            s.synthdefs.insert("target_synth".to_string());
            s.synthdefs.insert("source_synth".to_string());
            s.synthdef_inputs.insert(
                "target_synth".to_string(),
                vec![vibelang_dsp::InputPort::ar("carrier", input_channels)],
            );
            s.synthdef_outputs.insert(
                "source_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: source_channels,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );

            let group_node = s.alloc_node_id();
            let group_bus = s.alloc_audio_bus(2);
            s.groups.insert(
                group_id,
                GroupState {
                    id: group_id,
                    name: "main".to_string(),
                    parent: None,
                    node_id: group_node,
                    audio_bus: group_bus,
                    link_synth_node_id: None,
                    muted: false,
                    soloed: false,
                    params: ParamMap::new(),
                    output_bus: None,
                    output_channels: None,
                },
            );

            s.voices.insert(
                target_id,
                VoiceState {
                    id: target_id,
                    config: VoiceConfig::new("target", "target_synth", group_id),
                    active_nodes: vec![NodeId::new(9001)],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: Vec::new(),
                    input_buses: Vec::new(),
                },
            );

            let source_bus = s.alloc_audio_bus(source_channels);
            s.voices.insert(
                source_id,
                VoiceState {
                    id: source_id,
                    config: VoiceConfig::new("source", "source_synth", group_id),
                    active_nodes: Vec::new(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), source_bus)],
                    input_buses: Vec::new(),
                },
            );
        }

        (handler, backend, state, target_id, source_id)
    }

    fn input_route_map(target: VoiceId, srcs: Vec<InputRouteSrc>) -> InputRouteMap {
        InputRouteMap::from([((target, "carrier".to_string()), srcs)])
    }

    #[tokio::test]
    async fn finalize_input_route_adds_mono_voice_source_link() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );

        handler.finalize_input_routes(&desired).await.unwrap();

        let creates = backend.creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].def, "input_link_1");

        let s = state.read().await;
        let target_bus = s.voices.get(&target_id).unwrap().input_buses[0].1;
        let source_bus = s.voices.get(&source_id).unwrap().output_buses[0].1;
        assert_eq!(creates[0].in_bus, source_bus.raw() as f32);
        assert_eq!(creates[0].out_bus, target_bus.raw() as f32);
        assert!(s.input_route_synths.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Voice(source_id, "out".to_string())
        )));
        drop(s);

        assert_eq!(
            backend.sets(),
            vec![(
                NodeId::new(9001),
                "__in0".to_string(),
                target_bus.raw() as f32
            )]
        );
    }

    #[tokio::test]
    async fn finalize_input_route_adds_stereo_silent_link() {
        let (handler, backend, state, target_id, _source_id) =
            setup_named_input_fixture(2, 2).await;
        let desired = input_route_map(target_id, vec![InputRouteSrc::Silent]);

        handler.finalize_input_routes(&desired).await.unwrap();

        let creates = backend.creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].def, "input_link_2");

        let s = state.read().await;
        let silent_bus = s.silent_ar_bus.expect("silent bus allocated");
        let target_bus = s.voices.get(&target_id).unwrap().input_buses[0].1;
        assert_eq!(creates[0].in_bus, silent_bus.raw() as f32);
        assert_eq!(creates[0].out_bus, target_bus.raw() as f32);
    }

    #[tokio::test]
    async fn finalize_input_route_rejects_group_source_to_mono_input() {
        let (handler, backend, state, target_id, _source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(target_id, vec![InputRouteSrc::Group(GroupId::new(1))]);

        handler.finalize_input_routes(&desired).await.unwrap();

        assert_eq!(backend.synths_created(), 0);
        let s = state.read().await;
        assert!(s.input_routes.is_empty());
        assert!(s.input_route_synths.is_empty());
        assert!(s.voices.get(&target_id).unwrap().input_buses.is_empty());
    }

    #[tokio::test]
    async fn finalize_input_route_adds_stereo_group_source_link() {
        let (handler, backend, state, target_id, _source_id) =
            setup_named_input_fixture(2, 1).await;
        let source_group = GroupId::new(1);
        let desired = input_route_map(target_id, vec![InputRouteSrc::Group(source_group)]);

        handler.finalize_input_routes(&desired).await.unwrap();

        let creates = backend.creates();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].def, "input_link_2");

        let s = state.read().await;
        let group_bus = s.groups.get(&source_group).unwrap().audio_bus;
        let target_bus = s.voices.get(&target_id).unwrap().input_buses[0].1;
        assert_eq!(creates[0].in_bus, group_bus.raw() as f32);
        assert_eq!(creates[0].out_bus, target_bus.raw() as f32);
        assert!(s.input_route_synths.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Group(source_group)
        )));
    }

    #[tokio::test]
    async fn finalize_input_route_replace_frees_old_link_and_spawns_new() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let voice_route = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );
        handler.finalize_input_routes(&voice_route).await.unwrap();
        let old_node = state
            .read()
            .await
            .input_route_synths
            .get(&(
                target_id,
                "carrier".to_string(),
                InputRouteSrc::Voice(source_id, "out".to_string()),
            ))
            .copied()
            .unwrap();

        let silent_route = input_route_map(target_id, vec![InputRouteSrc::Silent]);
        handler.finalize_input_routes(&silent_route).await.unwrap();

        assert_eq!(backend.creates().len(), 2);
        assert!(backend.frees().contains(&old_node));
        let registry = state.read().await.input_route_synths.clone();
        assert!(!registry.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Voice(source_id, "out".to_string()),
        )));
        assert!(registry.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Silent,
        )));
    }

    #[tokio::test]
    async fn finalize_input_route_remove_disconnects_link() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );
        handler.finalize_input_routes(&desired).await.unwrap();

        let old_node = state
            .read()
            .await
            .input_route_synths
            .values()
            .copied()
            .next()
            .unwrap();

        handler
            .finalize_input_routes(&InputRouteMap::new())
            .await
            .unwrap();

        assert!(backend.frees().contains(&old_node));
        assert!(state.read().await.input_route_synths.is_empty());
        assert!(state.read().await.input_routes.is_empty());
    }

    #[tokio::test]
    async fn finalize_input_route_repeat_is_no_op() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );

        handler.finalize_input_routes(&desired).await.unwrap();
        let first_bus = state
            .read()
            .await
            .voices
            .get(&target_id)
            .unwrap()
            .input_buses[0]
            .1;
        handler.finalize_input_routes(&desired).await.unwrap();

        assert_eq!(backend.synths_created(), 1);
        assert_eq!(backend.nodes_freed(), 0);
        assert_eq!(
            state
                .read()
                .await
                .voices
                .get(&target_id)
                .unwrap()
                .input_buses[0]
                .1,
            first_bus
        );
    }

    #[tokio::test]
    async fn finalize_input_route_planning_failure_does_not_advance_state_and_retries() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "missing".to_string())],
        );

        handler.finalize_input_routes(&desired).await.unwrap();

        {
            let s = state.read().await;
            assert!(s.input_routes.is_empty());
            assert!(s.input_route_synths.is_empty());
            assert!(s.voices.get(&target_id).unwrap().input_buses.is_empty());
        }
        assert_eq!(backend.synths_created(), 0);

        {
            let mut s = state.write().await;
            let source_bus = s.voices.get(&source_id).unwrap().output_buses[0].1;
            s.voices
                .get_mut(&source_id)
                .unwrap()
                .output_buses
                .push(("missing".to_string(), source_bus));
            s.synthdef_outputs
                .get_mut("source_synth")
                .unwrap()
                .push(vibelang_dsp::OutputPort {
                    name: "missing".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                });
        }

        handler.finalize_input_routes(&desired).await.unwrap();

        let s = state.read().await;
        assert_eq!(s.input_routes, desired);
        assert!(s.input_route_synths.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Voice(source_id, "missing".to_string())
        )));
        assert_eq!(backend.synths_created(), 1);
    }

    #[tokio::test]
    async fn finalize_input_route_create_failure_does_not_advance_state_and_retries() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );

        backend.fail_next_create();
        let result = handler.finalize_input_routes(&desired).await;

        assert!(result.is_err());
        {
            let s = state.read().await;
            assert!(s.input_routes.is_empty());
            assert!(s.input_route_synths.is_empty());
        }
        assert_eq!(backend.synths_created(), 0);

        handler.finalize_input_routes(&desired).await.unwrap();

        let s = state.read().await;
        assert_eq!(s.input_routes, desired);
        assert!(s.input_route_synths.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Voice(source_id, "out".to_string())
        )));
        assert_eq!(backend.synths_created(), 1);
    }

    #[tokio::test]
    async fn finalize_input_route_set_param_failure_cleans_created_node_and_retries() {
        let (handler, backend, state, target_id, source_id) =
            setup_named_input_fixture(1, 1).await;
        let desired = input_route_map(
            target_id,
            vec![InputRouteSrc::Voice(source_id, "out".to_string())],
        );

        backend.fail_next_set();
        let result = handler.finalize_input_routes(&desired).await;

        assert!(result.is_err());
        let failed_node = backend.creates()[0].node;
        assert!(backend.frees().contains(&failed_node));
        {
            let s = state.read().await;
            assert!(s.input_routes.is_empty());
            assert!(s.input_route_synths.is_empty());
        }

        handler.finalize_input_routes(&desired).await.unwrap();

        let s = state.read().await;
        assert_eq!(s.input_routes, desired);
        assert!(s.input_route_synths.contains_key(&(
            target_id,
            "carrier".to_string(),
            InputRouteSrc::Voice(source_id, "out".to_string())
        )));
        assert_eq!(backend.synths_created(), 2);
    }

    // =========================================================================
    // v3 B3: multi-target Group fan-out — `.to(g_a).to(g_b)` installs both edges
    // =========================================================================

    #[tokio::test]
    async fn finalize_fan_out_spawns_one_mixer_per_group_edge() {
        // A single port routed to two distinct groups must spawn two mixer
        // synths (one per edge), each writing to the corresponding group's
        // audio bus.
        let (handler, backend, state, voice_id, port_name, dest_a) =
            setup_voice_in_group(2).await;

        let dest_b = GroupId::new(99);
        let bus_b = {
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
                    output_channels: None,
                },
            );
            bus
        };

        let mut diff = RouteDiff::default();
        diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_a),
        });
        diff.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_b),
        });

        handler.finalize(&diff).await.unwrap();

        assert_eq!(backend.synths_created(), 2, "one mixer per group edge");
        let creates = backend.creates();
        let bus_a = state
            .read()
            .await
            .groups
            .get(&dest_a)
            .unwrap()
            .audio_bus
            .0 as f32;
        let mut out_buses: Vec<f32> = creates.iter().map(|c| c.out_bus).collect();
        out_buses.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut want = vec![bus_a, bus_b.0 as f32];
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(out_buses, want);

        // Per-edge tracking in `route_synths`.
        let registry = state.read().await.route_synths.clone();
        assert!(registry
            .contains_key(&(voice_id, port_name.clone(), RouteDest::Group(dest_a))));
        assert!(registry.contains_key(&(voice_id, port_name, RouteDest::Group(dest_b))));
    }

    #[tokio::test]
    async fn finalize_fan_out_dropping_one_group_frees_only_that_mixer() {
        // After fan-out is in place, removing one of the two group edges
        // must free only the mixer for the dropped edge — the surviving
        // edge keeps its mixer node ID untouched.
        let (handler, backend, state, voice_id, port_name, dest_a) =
            setup_voice_in_group(2).await;

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
                    output_channels: None,
                },
            );
        }

        // Install both edges first.
        let mut add = RouteDiff::default();
        add.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_a),
        });
        add.additions.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_b),
        });
        handler.finalize(&add).await.unwrap();
        assert_eq!(backend.synths_created(), 2);

        let node_a = state
            .read()
            .await
            .route_synths
            .get(&(voice_id, port_name.clone(), RouteDest::Group(dest_a)))
            .copied()
            .unwrap();
        let node_b = state
            .read()
            .await
            .route_synths
            .get(&(voice_id, port_name.clone(), RouteDest::Group(dest_b)))
            .copied()
            .unwrap();

        // Drop only the edge to A — node_b must survive.
        let mut rm = RouteDiff::default();
        rm.removals.push(Route {
            voice_id,
            port_name: port_name.clone(),
            dest: RouteDest::Group(dest_a),
        });
        handler.finalize(&rm).await.unwrap();

        assert!(
            backend.frees().contains(&node_a),
            "mixer for dropped edge freed"
        );
        assert!(
            !backend.frees().contains(&node_b),
            "mixer for surviving edge must not be freed",
        );

        let registry = state.read().await.route_synths.clone();
        assert!(!registry
            .contains_key(&(voice_id, port_name.clone(), RouteDest::Group(dest_a))));
        assert_eq!(
            registry.get(&(voice_id, port_name, RouteDest::Group(dest_b))),
            Some(&node_b),
        );
    }

    // =========================================================================
    // Param-route diff + finalize_params — SET vs BEND (Multi-output v2 split)
    // =========================================================================

    /// Build the shared SET/BEND fixture: one source-voice per kr port plus
    /// one target voice with `active_target_nodes` synth nodes pre-spawned
    /// (the targets that finalize_params will issue `/n_map` against).
    async fn setup_split_fixture(
        n_sources: usize,
        active_target_nodes: usize,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        Vec<VoiceId>,
        Vec<BusId>,
        VoiceId,
        String,
        Vec<NodeId>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let voice_group_id = GroupId::new(1);
        let target_voice = VoiceId::new(100);
        let target_param = "cutoff".to_string();
        let mut source_voices = Vec::with_capacity(n_sources);
        let mut source_buses = Vec::with_capacity(n_sources);
        let mut target_nodes = Vec::with_capacity(active_target_nodes);
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
                    output_channels: None,
                },
            );

            for i in 0..n_sources {
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
                        input_buses: Vec::new(),
                    },
                );
                source_voices.push(vid);
                source_buses.push(bus);
            }

            for _ in 0..active_target_nodes {
                target_nodes.push(s.alloc_node_id());
            }
            let target_audio_bus = s.alloc_audio_bus(2);
            s.voices.insert(
                target_voice,
                VoiceState {
                    id: target_voice,
                    config: VoiceConfig::new("target", "ar_synth", voice_group_id),
                    active_nodes: target_nodes.clone(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), target_audio_bus)],
                    input_buses: Vec::new(),
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
            target_nodes,
        )
    }

    fn route_for_sources(
        sources: &[VoiceId],
        target_voice: VoiceId,
        target_param: &str,
    ) -> ParamRouteDiff {
        let mut diff = ParamRouteDiff::default();
        for sv in sources {
            diff.additions.push(ParamRoute {
                source_voice: *sv,
                source_port: "out".to_string(),
                target: ParamRouteTarget::Voice(target_voice),
                target_param: target_param.to_string(),
            });
        }
        diff
    }

    // ---------- diff_params is shared between SET and BEND ----------

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
                        .map(|(tv, tp)| {
                            (
                                ParamRouteTarget::Voice(VoiceId::new(*tv)),
                                (*tp).to_string(),
                            )
                        })
                        .collect::<Vec<(ParamRouteTarget, String)>>(),
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
    }

    #[test]
    fn diff_params_identical_returns_empty() {
        let routes = make_param_map(&[((10, "env"), &[(20, "cutoff"), (21, "amp")])]);
        let diff = RoutesHandler::<MockBackend>::diff_params(&routes, &routes);
        assert!(diff.is_empty());
    }

    // ---------- SET path ----------

    #[tokio::test]
    async fn set_finalize_single_source_spawns_modulate_1_summer_with_zero_baseline() {
        // Multi-output v3: SET path goes through `param_kr_modulate_1` with
        // baseline=0 and the per-source defaults scale=1 / offset=0, so the
        // source signal flows through unchanged — preserving the SET
        // "replace" semantic while sharing the BEND infrastructure.
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(1, 1).await;

        // Stamp a baseline value the SET summer must NOT use (SET pins
        // baseline=0 regardless of set_param).
        {
            let mut s = state.write().await;
            s.voices
                .get_mut(&target_voice)
                .unwrap()
                .config
                .params
                .insert(target_param.clone(), 9999.0);
        }

        let set_diff = route_for_sources(&sources, target_voice, &target_param);

        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_modulate_1")
            .collect();
        assert_eq!(summers.len(), 1, "SET N=1 must spawn modulate_1 summer");
        let summer = summers[0];
        assert_eq!(
            *summer.params.get("baseline").unwrap(),
            0.0,
            "SET pins baseline=0 regardless of set_param baseline",
        );
        assert_eq!(*summer.params.get("scale_a").unwrap(), 1.0);
        assert_eq!(*summer.params.get("offset_a").unwrap(), 0.0);
        assert_eq!(
            *summer.params.get("in_a").unwrap() as u32,
            source_buses[0].raw()
        );
        let intermediate = *summer.params.get("out_bus").unwrap() as u32;

        let maps = backend.maps();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].node, target_nodes[0]);
        assert_eq!(maps[0].param, target_param);
        assert_eq!(maps[0].bus, intermediate);

        let s = state.read().await;
        assert!(s
            .param_routes_set
            .contains_key(&(sources[0], "out".to_string())));
        assert!(s.param_routes_bend.is_empty());
        let recorded = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
            .expect("SET summer tracked");
        assert_eq!(recorded.node, summer.node);
        assert_eq!(recorded.arity(), 1);
        assert_eq!(recorded.sources[0].bus, source_buses[0]);
        assert_eq!(recorded.sources[0].scale, 1.0);
        assert_eq!(recorded.sources[0].offset, 0.0);
    }

    #[tokio::test]
    async fn set_finalize_drops_extra_sources_with_warning_uses_summer() {
        // Defence-in-depth: the script-time check rejects multi-source SET,
        // but if a misbehaving caller stuffs two SET sources at the same
        // target, finalize_params must drop the extras (warning) rather than
        // produce undefined behaviour. The runtime spawns a single
        // modulate_1 summer for the surviving source.
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(2, 1).await;

        let set_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def.starts_with("param_kr_modulate_"))
            .collect();
        assert_eq!(summers.len(), 1);
        assert_eq!(summers[0].def, "param_kr_modulate_1");

        // Summer reads the lower-id source bus (sort order is deterministic).
        let mut sorted: Vec<u32> = source_buses.iter().map(|b| b.raw()).collect();
        sorted.sort();
        assert_eq!(*summers[0].params.get("in_a").unwrap() as u32, sorted[0]);
        let intermediate = *summers[0].params.get("out_bus").unwrap() as u32;

        let maps = backend.maps();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].node, target_nodes[0]);
        assert_eq!(maps[0].bus, intermediate);

        // Both source-side keys are still recorded in param_routes_set.
        let s = state.read().await;
        assert_eq!(s.param_routes_set.len(), 2);
    }

    #[tokio::test]
    async fn set_removal_unmaps_target_and_frees_summer() {
        // Removed SET route: target unmapped via `/n_map -1`, summer node
        // freed.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(1, 1).await;

        // Stamp a baseline value the unmap should restore as a fallback.
        {
            let mut s = state.write().await;
            s.voices
                .get_mut(&target_voice)
                .unwrap()
                .config
                .params
                .insert(target_param.clone(), 1234.0);
        }

        let set_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let summer_node = {
            let s = state.read().await;
            s.param_summers
                .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
                .unwrap()
                .node
        };

        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(&rm, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        assert!(backend.frees().contains(&summer_node));
        let maps = backend.maps();
        let unmap = maps.last().unwrap();
        assert_eq!(unmap.node, target_nodes[0]);
        assert_eq!(unmap.bus, u32::MAX, "scsynth -1 unmap sentinel");

        let s = state.read().await;
        assert!(!s
            .param_routes_set
            .contains_key(&(sources[0], "out".to_string())));
        assert!(s.param_summers.is_empty());
    }

    // ---------- BEND path ----------

    #[tokio::test]
    async fn bend_finalize_single_source_spawns_modulate_1_summer_with_baseline() {
        // .modulate_by N=1: still spawns a summer (param_kr_modulate_1) so
        // the user's set_param value rides as `baseline` underneath the
        // single source. /n_map binds to the summer's intermediate bus.
        let (handler, backend, state, sources, source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(1, 1).await;

        // Stamp a baseline value before installing the bend route.
        {
            let mut s = state.write().await;
            s.voices
                .get_mut(&target_voice)
                .unwrap()
                .config
                .params
                .insert(target_param.clone(), 1500.0);
        }

        let bend_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&ParamRouteDiff::default(), &bend_diff, &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_modulate_1")
            .collect();
        assert_eq!(summers.len(), 1, "BEND N=1 must spawn modulate_1 summer");
        let summer = summers[0];
        assert_eq!(*summer.params.get("baseline").unwrap(), 1500.0);
        assert_eq!(*summer.params.get("in_a").unwrap() as u32, source_buses[0].raw());
        assert_eq!(
            *summer.params.get("scale_a").unwrap(),
            1.0,
            "default scale_a=1.0 wired even with no .scale() override",
        );
        assert_eq!(
            *summer.params.get("offset_a").unwrap(),
            0.0,
            "default offset_a=0.0 wired even with no .offset() override",
        );
        let intermediate = *summer.params.get("out_bus").unwrap() as u32;
        assert!(intermediate >= 1000, "intermediate bus from control pool");

        let maps = backend.maps();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].node, target_nodes[0]);
        assert_eq!(maps[0].bus, intermediate);

        let s = state.read().await;
        let recorded = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
            .expect("summer tracked");
        assert_eq!(recorded.node, summer.node);
        assert_eq!(recorded.bus.raw(), intermediate);
        assert_eq!(recorded.arity(), 1, "arity recorded as 1");
        assert_eq!(recorded.sources[0].bus, source_buses[0]);
        assert_eq!(recorded.sources[0].scale, 1.0);
        assert_eq!(recorded.sources[0].offset, 0.0);
    }

    #[tokio::test]
    async fn bend_finalize_multi_source_spawns_modulate_n_summer() {
        // .modulate_by N=3: param_kr_modulate_3 with baseline + in_a + in_b + in_c.
        let (handler, backend, _state, sources, source_buses, target_voice, target_param, _) =
            setup_split_fixture(3, 1).await;

        let bend_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&ParamRouteDiff::default(), &bend_diff, &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_modulate_3")
            .collect();
        assert_eq!(summers.len(), 1);
        let summer = summers[0];
        let mut sorted: Vec<u32> = source_buses.iter().map(|b| b.raw()).collect();
        sorted.sort();
        assert_eq!(*summer.params.get("in_a").unwrap() as u32, sorted[0]);
        assert_eq!(*summer.params.get("in_b").unwrap() as u32, sorted[1]);
        assert_eq!(*summer.params.get("in_c").unwrap() as u32, sorted[2]);
        // Default per-source scale/offset wired even at multi-arity.
        for letter in ['a', 'b', 'c'] {
            assert_eq!(
                *summer.params.get(&format!("scale_{}", letter)).unwrap(),
                1.0,
            );
            assert_eq!(
                *summer.params.get(&format!("offset_{}", letter)).unwrap(),
                0.0,
            );
        }
        // baseline defaults to 0 when no prior set_param was issued.
        assert_eq!(*summer.params.get("baseline").unwrap(), 0.0);
    }

    #[tokio::test]
    async fn bend_arity_change_respawns_summer_freeing_old_node_and_bus() {
        // 3-source bend → 2-source bend: old summer freed, new modulate_2
        // spawned, target rebound to the new intermediate bus, old
        // intermediate bus returned to the control-bus pool.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_node) =
            setup_split_fixture(3, 1).await;

        let bend_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&ParamRouteDiff::default(), &bend_diff, &ParamRouteDiff::default())
            .await
            .unwrap();

        let (old_summer_node, old_intermediate, old_arity) = {
            let s = state.read().await;
            let recorded = s
                .param_summers
                .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
                .unwrap();
            (recorded.node, recorded.bus, recorded.arity())
        };
        assert_eq!(old_arity, 3);

        // Drop one source.
        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(&ParamRouteDiff::default(), &rm, &ParamRouteDiff::default())
            .await
            .unwrap();

        assert!(backend.frees().contains(&old_summer_node));
        let creates = backend.creates();
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def.starts_with("param_kr_modulate_"))
            .collect();
        assert_eq!(summers.len(), 2);
        assert_eq!(summers[0].def, "param_kr_modulate_3");
        assert_eq!(summers[1].def, "param_kr_modulate_2");

        let new_intermediate = *summers[1].params.get("out_bus").unwrap() as u32;
        let last_map = backend.maps().last().unwrap().clone();
        assert_eq!(last_map.node, target_node[0]);
        assert_eq!(last_map.bus, new_intermediate);

        // Old intermediate bus is back in the pool — next alloc reuses it.
        let mut s = state.write().await;
        let reused = s.alloc_control_bus().raw();
        assert_eq!(reused, old_intermediate.raw(), "intermediate bus recycled");
    }

    #[tokio::test]
    async fn bend_remove_all_sources_frees_summer_and_unmaps() {
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(2, 1).await;

        // Stamp a baseline so the unmap restores it.
        {
            let mut s = state.write().await;
            s.voices
                .get_mut(&target_voice)
                .unwrap()
                .config
                .params
                .insert(target_param.clone(), 222.0);
        }

        let bend_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&ParamRouteDiff::default(), &bend_diff, &ParamRouteDiff::default())
            .await
            .unwrap();

        let summer_node = {
            let s = state.read().await;
            s.param_summers
                .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
                .unwrap()
                .node
        };

        let mut rm = ParamRouteDiff::default();
        for sv in &sources {
            rm.removals.push(ParamRoute {
                source_voice: *sv,
                source_port: "out".to_string(),
                target: ParamRouteTarget::Voice(target_voice),
                target_param: target_param.clone(),
            });
        }
        handler
            .finalize_params(&ParamRouteDiff::default(), &rm, &ParamRouteDiff::default())
            .await
            .unwrap();

        assert!(backend.frees().contains(&summer_node));
        let last_map = backend.maps().last().unwrap().clone();
        assert_eq!(last_map.node, target_nodes[0]);
        assert_eq!(last_map.bus, u32::MAX);

        let s = state.read().await;
        assert!(s.param_routes_bend.is_empty());
        assert!(s.param_summers.is_empty());
    }

    #[tokio::test]
    async fn bend_no_op_diff_preserves_existing_summer() {
        // Hot-reload signal: same source set, same target — diff is empty,
        // finalize short-circuits, the summer node + intermediate bus
        // outlive the call.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, _) =
            setup_split_fixture(2, 1).await;

        let bend_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&ParamRouteDiff::default(), &bend_diff, &ParamRouteDiff::default())
            .await
            .unwrap();

        let snapshot = {
            let s = state.read().await;
            s.param_summers.clone()
        };
        let creates_before = backend.creates().len();
        let frees_before = backend.frees().len();

        handler
            .finalize_params(&ParamRouteDiff::default(), &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let s = state.read().await;
        assert_eq!(s.param_summers, snapshot, "summer untouched on no-op diff");
        assert_eq!(backend.creates().len(), creates_before);
        assert_eq!(backend.frees().len(), frees_before);
    }

    #[tokio::test]
    async fn voice_delete_path_drains_both_set_and_bend_maps() {
        // Voice delete cleans both source-side AND target-side routes from
        // both maps.
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
            st.param_routes_set.insert(
                (s_voice, "env".to_string()),
                vec![(ParamRouteTarget::Voice(t0), "cutoff".to_string())],
            );
            st.param_routes_bend.insert(
                (s_voice, "lfo".to_string()),
                vec![(ParamRouteTarget::Voice(t1), "amp".to_string())],
            );
            st.param_routes_bend.insert(
                (s2_voice, "lfo".to_string()),
                vec![(ParamRouteTarget::Voice(t0), "freq".to_string())],
            );
        }

        let drained_t0 = {
            let mut st = state.write().await;
            st.take_voice_param_routes(t0)
        };
        assert!(drained_t0.is_empty(), "no source-side entries belong to t0");
        let s = state.read().await;
        assert!(
            !s.param_routes_set
                .contains_key(&(s_voice, "env".to_string())),
            "SET entry s→t0 cleaned up",
        );
        assert!(
            s.param_routes_bend
                .get(&(s_voice, "lfo".to_string()))
                .is_some(),
            "BEND s→t1 survives — different target",
        );
        assert!(
            !s.param_routes_bend
                .contains_key(&(s2_voice, "lfo".to_string())),
            "BEND s2→t0 entry pruned",
        );
    }

    // ==================== ar→param coercion (.to_param_audio) tests ====================

    /// Build a SET fixture where the source voice exposes an audio-rate
    /// port (synthdef "ar_kr_source_synth" with one ar port "out") plus
    /// a target voice with one active synth node. Mirrors
    /// `setup_split_fixture` shape.
    async fn setup_ar_source_fixture(
        n_ar_sources: usize,
        active_target_nodes: usize,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        Vec<VoiceId>,
        Vec<BusId>,
        VoiceId,
        String,
        Vec<NodeId>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let voice_group_id = GroupId::new(1);
        let target_voice = VoiceId::new(200);
        let target_param = "cutoff".to_string();
        let mut source_voices = Vec::with_capacity(n_ar_sources);
        let mut source_buses = Vec::with_capacity(n_ar_sources);
        let mut target_nodes = Vec::with_capacity(active_target_nodes);
        {
            let mut s = state.write().await;
            s.synthdefs.insert("ar_source_synth".to_string());
            s.synthdef_outputs.insert(
                "ar_source_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );
            s.synthdefs.insert("ar_target_synth".to_string());
            s.synthdef_outputs.insert(
                "ar_target_synth".to_string(),
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
                    output_channels: None,
                },
            );

            for i in 0..n_ar_sources {
                let vid = VoiceId::new(50 + i as u32);
                let ar_bus = s.alloc_audio_bus(1);
                s.voices.insert(
                    vid,
                    VoiceState {
                        id: vid,
                        config: VoiceConfig::new(
                            &format!("arsrc{}", i),
                            "ar_source_synth",
                            voice_group_id,
                        ),
                        active_nodes: Vec::new(),
                        note_nodes: HashMap::new(),
                        round_robin_position: 0,
                        pending_params: HashMap::new(),
                        output_buses: vec![("out".to_string(), ar_bus)],
                        input_buses: Vec::new(),
                    },
                );
                source_voices.push(vid);
                source_buses.push(ar_bus);
            }

            for _ in 0..active_target_nodes {
                target_nodes.push(s.alloc_node_id());
            }
            let target_audio_bus = s.alloc_audio_bus(2);
            s.voices.insert(
                target_voice,
                VoiceState {
                    id: target_voice,
                    config: VoiceConfig::new(
                        "ar_target",
                        "ar_target_synth",
                        voice_group_id,
                    ),
                    active_nodes: target_nodes.clone(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), target_audio_bus)],
                    input_buses: Vec::new(),
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
            target_nodes,
        )
    }

    #[tokio::test]
    async fn ar_to_param_set_spawns_a2k_adapter_and_summer_and_n_maps() {
        // .to_param_audio: ar source coerced to kr via a2k_adapter_1, then
        // routed into the same summer infrastructure as kr SET.
        let (
            handler,
            backend,
            state,
            sources,
            source_buses,
            target_voice,
            target_param,
            target_nodes,
        ) = setup_ar_source_fixture(1, 1).await;

        let set_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let adapters: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "a2k_adapter_1")
            .collect();
        assert_eq!(
            adapters.len(),
            1,
            "exactly one a2k adapter spawned per (source, port)",
        );
        // Adapter reads the source's audio bus and writes a kr bus.
        let adapter = adapters[0];
        assert_eq!(*adapter.params.get("in_bus").unwrap() as u32, source_buses[0].raw());
        let adapter_kr_bus = *adapter.params.get("out_bus").unwrap() as u32;
        assert!(
            adapter_kr_bus >= 1000,
            "adapter writes to a control-bus-pool kr bus (got {})",
            adapter_kr_bus,
        );

        // Summer reads the adapter's kr bus, not the source's audio bus.
        let summers: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "param_kr_modulate_1")
            .collect();
        assert_eq!(summers.len(), 1, "SET path still goes through modulate_1");
        let summer = summers[0];
        assert_eq!(
            *summer.params.get("in_a").unwrap() as u32,
            adapter_kr_bus,
            "summer in_a reads the adapter's kr bus, not the source audio bus",
        );
        assert_eq!(*summer.params.get("baseline").unwrap(), 0.0, "SET pins baseline=0");

        let intermediate = *summer.params.get("out_bus").unwrap() as u32;
        let maps = backend.maps();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].node, target_nodes[0]);
        assert_eq!(maps[0].param, target_param);
        assert_eq!(maps[0].bus, intermediate);

        // State carries the adapter so future diffs can locate it.
        let s = state.read().await;
        let (adapter_node, adapter_bus) = s
            .ar_to_kr_adapters
            .get(&(sources[0], "out".to_string()))
            .copied()
            .expect("adapter recorded in state");
        assert_eq!(adapter_node, adapter.node);
        assert_eq!(adapter_bus.raw(), adapter_kr_bus);
        // The summer's source bus matches the adapter's kr bus too.
        let recorded_summer = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
            .expect("summer recorded");
        assert_eq!(recorded_summer.sources[0].bus.raw(), adapter_kr_bus);
        assert_eq!(recorded_summer.sources[0].scale, 1.0);
        assert_eq!(recorded_summer.sources[0].offset, 0.0);
    }

    #[tokio::test]
    async fn ar_to_param_removal_frees_summer_and_adapter() {
        // Tear down the only ar→param route on a source: summer + intermediate
        // bus go back to the pool, adapter + its kr bus too.
        let (handler, backend, state, sources, _src_buses, target_voice, target_param, _target_nodes) =
            setup_ar_source_fixture(1, 1).await;

        let set_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let (adapter_node, summer_node, adapter_bus, summer_bus) = {
            let s = state.read().await;
            let (adapter_node, adapter_bus) = s
                .ar_to_kr_adapters
                .get(&(sources[0], "out".to_string()))
                .copied()
                .unwrap();
            let summer = s
                .param_summers
                .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
                .unwrap();
            (adapter_node, summer.node, adapter_bus, summer.bus)
        };
        assert_ne!(adapter_node, summer_node, "distinct nodes");
        assert_ne!(adapter_bus.raw(), summer_bus.raw(), "distinct kr buses");

        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(&rm, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let frees = backend.frees();
        assert!(frees.contains(&summer_node), "summer freed");
        assert!(frees.contains(&adapter_node), "adapter freed");

        let s = state.read().await;
        assert!(s.ar_to_kr_adapters.is_empty(), "adapter cleared from state");
        assert!(s.param_summers.is_empty(), "summer cleared from state");
    }

    #[tokio::test]
    async fn ar_to_param_adapter_shared_across_targets_from_one_source() {
        // One ar source feeding two distinct targets via .to_param_audio:
        // ONE adapter handles both. Each target still gets its own summer
        // + intermediate bus.
        let (handler, backend, state, sources, _src_buses, _, _, _) =
            setup_ar_source_fixture(1, 0).await;

        // Add a second target voice with its own active node.
        let target_a = VoiceId::new(300);
        let target_b = VoiceId::new(301);
        let voice_group_id = GroupId::new(1);
        let (target_a_node, target_b_node) = {
            let mut s = state.write().await;
            s.synthdefs.insert("dual_target_synth".to_string());
            s.synthdef_outputs.insert(
                "dual_target_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 2,
                    rate: vibelang_dsp::PortRate::Ar,
                }],
            );
            let a_node = s.alloc_node_id();
            let b_node = s.alloc_node_id();
            let bus_a = s.alloc_audio_bus(2);
            let bus_b = s.alloc_audio_bus(2);
            s.voices.insert(
                target_a,
                VoiceState {
                    id: target_a,
                    config: VoiceConfig::new("ta", "dual_target_synth", voice_group_id),
                    active_nodes: vec![a_node],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_a)],
                    input_buses: Vec::new(),
                },
            );
            s.voices.insert(
                target_b,
                VoiceState {
                    id: target_b,
                    config: VoiceConfig::new("tb", "dual_target_synth", voice_group_id),
                    active_nodes: vec![b_node],
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), bus_b)],
                    input_buses: Vec::new(),
                },
            );
            (a_node, b_node)
        };

        let mut diff = ParamRouteDiff::default();
        diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_a),
            target_param: "x".to_string(),
        });
        diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_b),
            target_param: "y".to_string(),
        });
        // Both targets point at distinct (target_voice, target_param) pairs
        // so the SET multi-source guard is not tripped — the same source
        // port fans out into two SET targets, which is allowed.
        handler
            .finalize_params(&diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        let n_adapters = creates
            .iter()
            .filter(|c| c.def == "a2k_adapter_1")
            .count();
        let n_summers = creates
            .iter()
            .filter(|c| c.def.starts_with("param_kr_modulate_"))
            .count();
        assert_eq!(
            n_adapters, 1,
            "shared adapter for one (source, port) — got {}",
            n_adapters,
        );
        assert_eq!(n_summers, 2, "one summer per target");

        // /n_map issued for both target nodes.
        let map_nodes: HashSet<NodeId> = backend.maps().iter().map(|m| m.node).collect();
        assert!(map_nodes.contains(&target_a_node));
        assert!(map_nodes.contains(&target_b_node));

        // Both summers read from the same adapter bus.
        let s = state.read().await;
        let (_, adapter_bus) = *s
            .ar_to_kr_adapters
            .get(&(sources[0], "out".to_string()))
            .unwrap();
        let summer_a = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_a), "x".to_string()))
            .unwrap();
        let summer_b = s
            .param_summers
            .get(&(ParamRouteTarget::Voice(target_b), "y".to_string()))
            .unwrap();
        assert_eq!(summer_a.sources[0].bus.raw(), adapter_bus.raw());
        assert_eq!(summer_b.sources[0].bus.raw(), adapter_bus.raw());
    }

    #[tokio::test]
    async fn kr_source_to_param_does_not_spawn_adapter() {
        // Sanity check the rate-detection: a kr source going into the SET
        // path must NOT trigger adapter allocation.
        let (handler, backend, state, sources, source_buses, target_voice, target_param, _) =
            setup_split_fixture(1, 1).await;

        let set_diff = route_for_sources(&sources, target_voice, &target_param);
        handler
            .finalize_params(&set_diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let creates = backend.creates();
        assert!(
            creates.iter().all(|c| c.def != "a2k_adapter_1"),
            "kr source must not spawn an a2k adapter",
        );
        let summer = creates
            .iter()
            .find(|c| c.def == "param_kr_modulate_1")
            .expect("kr SET still spawns the summer");
        // Summer reads the source's own kr bus directly.
        assert_eq!(*summer.params.get("in_a").unwrap() as u32, source_buses[0].raw());

        let s = state.read().await;
        assert!(s.ar_to_kr_adapters.is_empty());
    }

    // ---------- TRIGGER path (B2.c) ----------

    /// Build a fixture with `n_sources` trigger-rate sources plus one target
    /// voice. Mirrors [`setup_split_fixture`] but registers the source
    /// synthdef as Tr-rate and uses control buses (Tr ports share kr storage).
    async fn setup_trigger_fixture(
        n_sources: usize,
        active_target_nodes: usize,
    ) -> (
        RoutesHandler<MockBackend>,
        Arc<MockBackend>,
        Arc<RwLock<State>>,
        Vec<VoiceId>,
        Vec<BusId>,
        VoiceId,
        String,
        Vec<NodeId>,
    ) {
        let backend = Arc::new(MockBackend::new());
        let state = Arc::new(RwLock::new(State::default()));
        let handler = RoutesHandler::new(backend.clone(), state.clone());

        let voice_group_id = GroupId::new(1);
        let target_voice = VoiceId::new(200);
        let target_param = "gate".to_string();
        let mut source_voices = Vec::with_capacity(n_sources);
        let mut source_buses = Vec::with_capacity(n_sources);
        let mut target_nodes = Vec::with_capacity(active_target_nodes);
        {
            let mut s = state.write().await;
            s.synthdefs.insert("tr_synth".to_string());
            s.synthdef_outputs.insert(
                "tr_synth".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "out".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Tr,
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
                    output_channels: None,
                },
            );

            for i in 0..n_sources {
                let vid = VoiceId::new(20 + i as u32);
                let kr_bus = s.alloc_control_bus();
                let bus = BusId::new(kr_bus.raw());
                s.voices.insert(
                    vid,
                    VoiceState {
                        id: vid,
                        config: VoiceConfig::new(
                            &format!("trsrc{}", i),
                            "tr_synth",
                            voice_group_id,
                        ),
                        active_nodes: Vec::new(),
                        note_nodes: HashMap::new(),
                        round_robin_position: 0,
                        pending_params: HashMap::new(),
                        output_buses: vec![("out".to_string(), bus)],
                        input_buses: Vec::new(),
                    },
                );
                source_voices.push(vid);
                source_buses.push(bus);
            }

            for _ in 0..active_target_nodes {
                target_nodes.push(s.alloc_node_id());
            }
            let target_audio_bus = s.alloc_audio_bus(2);
            s.voices.insert(
                target_voice,
                VoiceState {
                    id: target_voice,
                    config: VoiceConfig::new("kick", "ar_synth", voice_group_id),
                    active_nodes: target_nodes.clone(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("out".to_string(), target_audio_bus)],
                    input_buses: Vec::new(),
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
            target_nodes,
        )
    }

    #[tokio::test]
    async fn trigger_finalize_single_source_spawns_link_and_maps_target() {
        // .to_trigger N=1: spawn `port_tr_to_param_link_1` (NOT a
        // param_kr_modulate summer), wire in_bus = source's Tr bus, out_bus =
        // intermediate kr bus, and /n_map every active target node to the
        // intermediate bus.
        let (
            handler,
            backend,
            state,
            sources,
            source_buses,
            target_voice,
            target_param,
            target_nodes,
        ) = setup_trigger_fixture(1, 2).await;

        let mut trigger_diff = ParamRouteDiff::default();
        trigger_diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });

        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &trigger_diff,
            )
            .await
            .unwrap();

        let creates = backend.creates();
        let links: Vec<&CreateSynthCall> = creates
            .iter()
            .filter(|c| c.def == "port_tr_to_param_link_1")
            .collect();
        assert_eq!(
            links.len(),
            1,
            "TRIGGER N=1 must spawn exactly one port_tr_to_param_link_1",
        );
        let link = links[0];
        assert_eq!(
            *link.params.get("in_bus").unwrap() as u32,
            source_buses[0].raw(),
            "in_bus wired to the source's Tr port bus",
        );
        let intermediate = *link.params.get("out_bus").unwrap() as u32;
        assert!(intermediate >= 1000, "intermediate from control-bus pool");

        // No summer should have been spawned for this target.
        assert!(
            !creates.iter().any(|c| c.def.starts_with("param_kr_modulate_")),
            "TRIGGER must not spawn a kr summer",
        );

        // /n_map issued for every active target node, all pointing at the
        // link's intermediate bus.
        let maps = backend.maps();
        assert_eq!(maps.len(), 2, "one /n_map per active target node");
        for (i, m) in maps.iter().enumerate() {
            assert_eq!(m.node, target_nodes[i]);
            assert_eq!(m.param, target_param);
            assert_eq!(m.bus, intermediate);
        }

        // State carries the link entry and source-side route entry.
        let s = state.read().await;
        let (link_node, link_bus) = s
            .param_triggers
            .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
            .expect("trigger link tracked")
            .clone();
        assert_eq!(link_node, link.node);
        assert_eq!(link_bus.raw(), intermediate);
        assert!(s
            .param_routes_trigger
            .contains_key(&(sources[0], "out".to_string())));
        // No summer state.
        assert!(s.param_summers.is_empty());
    }

    #[tokio::test]
    async fn trigger_removal_unmaps_target_and_frees_link() {
        // Removing the only TRIGGER source for a target: link node freed,
        // intermediate bus returned to the pool, target /n_map -1'd, and the
        // baseline value (if any) restored.
        let (
            handler,
            backend,
            state,
            sources,
            _source_buses,
            target_voice,
            target_param,
            target_nodes,
        ) = setup_trigger_fixture(1, 1).await;

        // Stamp a baseline value so the unmap restores it.
        {
            let mut s = state.write().await;
            s.voices
                .get_mut(&target_voice)
                .unwrap()
                .config
                .params
                .insert(target_param.clone(), 0.5);
        }

        let mut trigger_diff = ParamRouteDiff::default();
        trigger_diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &trigger_diff,
            )
            .await
            .unwrap();

        let (link_node, link_bus) = {
            let s = state.read().await;
            *s.param_triggers
                .get(&(ParamRouteTarget::Voice(target_voice), target_param.clone()))
                .unwrap()
        };

        // Remove the trigger route.
        let mut rm = ParamRouteDiff::default();
        rm.removals.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &rm,
            )
            .await
            .unwrap();

        assert!(backend.frees().contains(&link_node));
        let last_map = backend.maps().last().unwrap().clone();
        assert_eq!(last_map.node, target_nodes[0]);
        assert_eq!(last_map.bus, u32::MAX, "scsynth -1 unmap sentinel");

        {
            let s = state.read().await;
            assert!(
                !s.param_routes_trigger
                    .contains_key(&(sources[0], "out".to_string())),
                "source-side route entry drained on removal",
            );
            assert!(s.param_triggers.is_empty(), "trigger state cleared");
        }

        // Intermediate bus is back in the pool — next alloc reuses it.
        let mut s = state.write().await;
        let reused = s.alloc_control_bus().raw();
        assert_eq!(reused, link_bus.raw(), "intermediate bus recycled");
    }

    #[tokio::test]
    async fn trigger_no_op_diff_preserves_existing_link() {
        // Hot-reload signal: same source set, same target — diff is empty,
        // finalize short-circuits, the link node + intermediate bus outlive
        // the call.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, _) =
            setup_trigger_fixture(1, 1).await;

        let mut trigger_diff = ParamRouteDiff::default();
        trigger_diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &trigger_diff,
            )
            .await
            .unwrap();

        let snapshot = state.read().await.param_triggers.clone();
        let creates_before = backend.creates().len();
        let frees_before = backend.frees().len();

        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
            )
            .await
            .unwrap();

        assert_eq!(backend.creates().len(), creates_before);
        assert_eq!(backend.frees().len(), frees_before);
        let after = state.read().await.param_triggers.clone();
        assert_eq!(after, snapshot);
    }

    #[tokio::test]
    async fn cross_verb_runtime_conflict_set_and_trigger_unmaps() {
        // Defence-in-depth: if a script-time bug puts the same target in both
        // SET and TRIGGER maps, finalize logs a warning and unmaps rather
        // than producing undefined behaviour.
        let (
            handler,
            backend,
            state,
            sources,
            _source_buses,
            target_voice,
            target_param,
            target_nodes,
        ) = setup_trigger_fixture(1, 1).await;

        // Add a kr source that lives in the SET map alongside the TRIGGER
        // entry.
        let kr_source_id = VoiceId::new(99);
        {
            let mut s = state.write().await;
            s.synthdefs.insert("conflicting_kr".to_string());
            s.synthdef_outputs.insert(
                "conflicting_kr".to_string(),
                vec![vibelang_dsp::OutputPort {
                    name: "env".to_string(),
                    channels: 1,
                    rate: vibelang_dsp::PortRate::Kr,
                }],
            );
            let kr_bus = s.alloc_control_bus();
            let voice_group_id = GroupId::new(1);
            s.voices.insert(
                kr_source_id,
                VoiceState {
                    id: kr_source_id,
                    config: VoiceConfig::new(
                        "kr_src_conflict",
                        "conflicting_kr",
                        voice_group_id,
                    ),
                    active_nodes: Vec::new(),
                    note_nodes: HashMap::new(),
                    round_robin_position: 0,
                    pending_params: HashMap::new(),
                    output_buses: vec![("env".to_string(), BusId::new(kr_bus.raw()))],
                    input_buses: Vec::new(),
                },
            );
            s.param_routes_set.insert(
                (kr_source_id, "env".to_string()),
                vec![(ParamRouteTarget::Voice(target_voice), target_param.clone())],
            );
            s.param_routes_trigger.insert(
                (sources[0], "out".to_string()),
                vec![(ParamRouteTarget::Voice(target_voice), target_param.clone())],
            );
        }

        // Drive the planner with an addition pointed at the conflicting target.
        let mut trigger_diff = ParamRouteDiff::default();
        trigger_diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(
                &ParamRouteDiff::default(),
                &ParamRouteDiff::default(),
                &trigger_diff,
            )
            .await
            .unwrap();

        let last_map = backend.maps().last().unwrap().clone();
        assert_eq!(last_map.node, target_nodes[0]);
        assert_eq!(
            last_map.bus,
            u32::MAX,
            "summer/trigger cross-verb conflict unmaps target",
        );
    }

    #[tokio::test]
    async fn cross_verb_runtime_conflict_logs_and_unmaps() {
        // Defence-in-depth: a script-time bug shouldn't blow up the runtime.
        // If both SET and BEND maps end up with the same target, finalize
        // logs a warning and unmaps the target rather than producing
        // undefined behaviour.
        let (handler, backend, state, sources, _source_buses, target_voice, target_param, target_nodes) =
            setup_split_fixture(2, 1).await;

        // Manually install entries in both maps (simulating a script-time
        // validation slip).
        {
            let mut s = state.write().await;
            s.param_routes_set.insert(
                (sources[0], "out".to_string()),
                vec![(ParamRouteTarget::Voice(target_voice), target_param.clone())],
            );
            s.param_routes_bend.insert(
                (sources[1], "out".to_string()),
                vec![(ParamRouteTarget::Voice(target_voice), target_param.clone())],
            );
        }

        // Pass an addition for one of the edges as a SET so the planner walks
        // the affected target and sees both maps populated.
        let mut diff = ParamRouteDiff::default();
        diff.additions.push(ParamRoute {
            source_voice: sources[0],
            source_port: "out".to_string(),
            target: ParamRouteTarget::Voice(target_voice),
            target_param: target_param.clone(),
        });
        handler
            .finalize_params(&diff, &ParamRouteDiff::default(), &ParamRouteDiff::default())
            .await
            .unwrap();

        let last_map = backend.maps().last().unwrap().clone();
        assert_eq!(last_map.node, target_nodes[0]);
        assert_eq!(last_map.bus, u32::MAX, "cross-verb conflict unmaps");
    }
}
