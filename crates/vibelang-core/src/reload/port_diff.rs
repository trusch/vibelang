//! Port-set reconciliation for synthdef edits during reload.
//!
//! When a synthdef is re-declared with a different port set (port added,
//! removed, or renamed), we have to reconcile three pieces of runtime state
//! in lockstep:
//!
//! 1. The voice's `output_buses` (one [`BusId`] chunk per port).
//! 2. [`State::synthdef_outputs`] — the registered port set the rest of the
//!    runtime queries via [`State::synthdef_outputs`](crate::state::State::synthdef_outputs).
//! 3. The script's [`RouteMap`] — `(voice_id, port_name)` keys for ports that
//!    no longer exist must be dropped so [`RoutesHandler::finalize`] frees
//!    their mixer synths instead of leaving them reading from a freed bus.
//!
//! Routes are name-keyed (Story 6a), so a body-only edit that preserves the
//! port set leaves every route entry intact and only the synth nodes
//! recreate. Renames are deliberately treated as remove + add — guessing
//! intent on a rename is worse than a clear warning that names the dropped
//! route so the user can re-route under the new name.
//!
//! [`RoutesHandler::finalize`]: crate::handlers::RoutesHandler::finalize
//! [`BusId`]: crate::types::BusId

use crate::handlers::{RouteDest, RouteMap};
use crate::state::State;
use crate::types::{BusId, ControlBusId, VoiceId};
use std::collections::HashMap;
use vibelang_dsp::{OutputPort, PortRate};

/// Diff between two port sets, keyed by port name.
///
/// Order-insensitive — only the set of names matters. A rename surfaces as a
/// pair of (`removed`, `added`) entries; a body-only edit leaves both lists
/// empty.
///
/// Story 8: a kept name whose `rate` changed (ar↔kr) is treated structurally
/// as remove + add — the underlying bus comes from a different allocator, so
/// the only correct path is to free the old-rate bus and allocate a new one.
/// The port appears in `removed` carrying its old rate and in `added` carrying
/// its new rate; the helper [`PortSetDiff::rate_changes`] surfaces the pair
/// for callers that want to log a "rate changed" warning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortSetDiff {
    /// Ports present in `new` but not in `old`, OR present in both with a
    /// different rate (the new-rate side).
    pub added: Vec<OutputPort>,
    /// Ports present in `old` but not in `new`, OR present in both with a
    /// different rate (the old-rate side). Carries the channel count and rate
    /// so the matching bus chunk can be returned to the right allocator.
    pub removed: Vec<OutputPort>,
    /// Ports present under the same name AND rate in both. Carries the *new*
    /// channel count; a channel-count change on a kept name is out of scope
    /// here (Story 6a treats it as the same routable destination).
    pub kept: Vec<OutputPort>,
}

impl PortSetDiff {
    /// True when the port set is identical (nothing added or removed).
    pub fn is_unchanged(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    /// Names that appear in both `removed` and `added` — i.e. ports whose
    /// rate flipped between script versions. Returned as
    /// `(name, old_rate, new_rate)` so callers can format a warning that
    /// names the transition.
    pub fn rate_changes(&self) -> Vec<(String, PortRate, PortRate)> {
        let added_by_name: HashMap<&str, &OutputPort> =
            self.added.iter().map(|p| (p.name.as_str(), p)).collect();
        self.removed
            .iter()
            .filter_map(|old_p| {
                added_by_name
                    .get(old_p.name.as_str())
                    .map(|new_p| (old_p.name.clone(), old_p.rate, new_p.rate))
            })
            .collect()
    }
}

/// Compute the port-set diff, name-keyed and order-insensitive.
///
/// Same name + same rate → `kept`. Same name + different rate → split into
/// `removed` (old-rate) + `added` (new-rate). Name-only mismatches surface as
/// a single-sided entry.
pub fn diff_port_set(old: &[OutputPort], new: &[OutputPort]) -> PortSetDiff {
    let old_by_name: HashMap<&str, &OutputPort> =
        old.iter().map(|p| (p.name.as_str(), p)).collect();
    let new_by_name: HashMap<&str, &OutputPort> =
        new.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut added = Vec::new();
    let mut kept = Vec::new();
    for p in new {
        match old_by_name.get(p.name.as_str()) {
            Some(old_p) if old_p.rate == p.rate => kept.push(p.clone()),
            // Same name, different rate — surface as the add half of a
            // remove + add pair. The remove half is emitted in the next loop.
            Some(_) => added.push(p.clone()),
            None => added.push(p.clone()),
        }
    }

    let mut removed = Vec::new();
    for p in old {
        match new_by_name.get(p.name.as_str()) {
            Some(new_p) if new_p.rate == p.rate => {} // truly kept
            // Same name, different rate — emit the remove half.
            Some(_) => removed.push(p.clone()),
            None => removed.push(p.clone()),
        }
    }

    PortSetDiff {
        added,
        removed,
        kept,
    }
}

/// Outcome of [`reconcile_voice_ports`].
///
/// `dropped_routes` lists `(voice_id, port_name)` keys that were stripped
/// from the supplied [`RouteMap`] because the underlying port no longer
/// exists. `dropped_param_routes` lists source-side
/// `(voice_id, port_name)` keys that were stripped from any of
/// [`State::param_routes_set`], [`State::param_routes_bend`], or
/// [`State::param_routes_trigger`] for the same reason — typically a kr
/// or tr port that was either removed outright or rate-flipped to ar. Tr
/// ports share the kr control-bus path and host Param routes the same way
/// (B2.b), so kr↔tr and kr/tr↔ar flips both surface here. The caller is
/// expected to log a warning naming each dropped route so the user can
/// clean up their script; [`PortReconcile::rate_changes`] separately
/// surfaces names whose rate flipped, which the reconcile already logs at
/// warn level.
#[derive(Clone, Debug, Default)]
pub struct PortReconcile {
    /// The computed port-set diff (added/removed/kept).
    pub diff: PortSetDiff,
    /// Route keys removed from the route map because their port disappeared
    /// or its rate flipped.
    pub dropped_routes: Vec<(VoiceId, String)>,
    /// Source-side Param-route keys removed from
    /// [`State::param_routes_set`] and/or [`State::param_routes_bend`]
    /// because the source port disappeared or its rate flipped to ar (a kr
    /// port hosts Param routes; an ar port cannot).
    pub dropped_param_routes: Vec<(VoiceId, String)>,
    /// Route keys inserted with `RouteDest::Muted` for newly added non-`out`
    /// ports per Story 5's silent-by-default rule.
    pub default_muted: Vec<(VoiceId, String)>,
    /// Ports whose rate flipped between script versions. Mirrors
    /// [`PortSetDiff::rate_changes`] but is recorded on the reconcile result
    /// for tests and callers that want a typed handle on the transition.
    pub rate_changes: Vec<(String, PortRate, PortRate)>,
}

fn rate_label(r: PortRate) -> &'static str {
    match r {
        PortRate::Ar => "ar",
        PortRate::Kr => "kr",
        PortRate::Tr => "tr",
    }
}

/// Reconcile a voice's bus allocations and route entries to match `new_ports`.
///
/// Mutates:
/// - `state.voices[voice_id].output_buses` — drops removed ports and appends
///   newly allocated ones. Ar ports go through the audio-bus allocator; kr
///   ports go through the control-bus allocator. The rate is taken from the
///   port's own `rate` field, so a rate flip (ar↔kr, see Story 8) frees the
///   old-rate bus through the matching freer and allocates the new-rate bus
///   through the matching allocator.
/// - The audio bus allocator (via [`State::free_audio_bus`] /
///   [`State::alloc_audio_bus`]) and the control bus allocator (via
///   [`State::free_control_bus`] / [`State::alloc_control_bus`]).
/// - `state.synthdef_outputs[voice.synthdef]` — overwritten with the new
///   set so subsequent [`State::synthdef_outputs`](crate::state::State::synthdef_outputs)
///   lookups see the up-to-date shape.
/// - `routes` — entries whose port name is in `diff.removed` are dropped;
///   newly added non-`out` ports get a default `RouteDest::Muted` entry so
///   the route diff against `current_routes` will spawn no mixer synth.
/// - `state.param_routes_set` and `state.param_routes_bend` — source-side
///   entries for any removed kr port are dropped from both maps, since
///   the source control bus has just been freed.
///
/// A rate flip on a kept name (ar↔kr) is treated as a structural remove +
/// add: the diff lists the port in both `removed` (with old rate) and
/// `added` (with new rate); a `tracing::warn!` is emitted naming the
/// transition and any routes that were dropped because of it.
///
/// Returns the diff plus the route mutations. A no-op return (no added /
/// removed ports) means the synthdef body changed but the port set did not,
/// and every existing route entry has been preserved.
pub fn reconcile_voice_ports(
    state: &mut State,
    voice_id: VoiceId,
    new_ports: &[OutputPort],
    routes: &mut RouteMap,
) -> PortReconcile {
    let synthdef = match state.voices.get(&voice_id) {
        Some(v) => v.config.synthdef.clone(),
        None => {
            tracing::warn!(
                "reconcile_voice_ports: voice {:?} not found, skipping",
                voice_id
            );
            return PortReconcile::default();
        }
    };
    let old_ports = state.synthdef_outputs(&synthdef);
    let diff = diff_port_set(&old_ports, new_ports);

    if diff.is_unchanged() {
        // Body-only edit: keep the registry fresh in case channel widths or
        // metadata shifted on a kept port, but do not touch buses or routes.
        state
            .synthdef_outputs
            .insert(synthdef, new_ports.to_vec());
        return PortReconcile {
            diff,
            ..Default::default()
        };
    }

    // ---- Free buses and drop routes for removed ports --------------------
    //
    // Each removed entry carries the *old* rate, so we dispatch to the
    // matching allocator: ar ports return their chunk to the audio-bus free
    // list with the original channel width, kr/tr ports return a single ID
    // to the control-bus free list (Tr ports share the kr storage path).
    // RouteMap entries belong to ar ports; kr and tr ports' Param routes
    // live in `state.param_routes_set` / `state.param_routes_bend` /
    // `state.param_routes_trigger` keyed at the source side, so we drain
    // those too — otherwise a kr/tr→ar flip would leave a dangling source
    // key pointing at a freed control bus.
    let mut dropped_routes = Vec::new();
    let mut dropped_param_routes = Vec::new();
    for port in &diff.removed {
        let bus = state
            .voices
            .get(&voice_id)
            .and_then(|v| {
                v.output_buses
                    .iter()
                    .find(|(n, _)| n == &port.name)
                    .map(|(_, b)| *b)
            });
        if let Some(bus) = bus {
            match port.rate {
                PortRate::Ar => state.free_audio_bus(bus, port.channels),
                PortRate::Kr | PortRate::Tr => {
                    state.free_control_bus(ControlBusId::new(bus.raw()));
                }
            }
        }
        if let Some(voice) = state.voices.get_mut(&voice_id) {
            voice.output_buses.retain(|(n, _)| n != &port.name);
        }
        let key = (voice_id, port.name.clone());
        if routes.remove(&key).is_some() {
            dropped_routes.push(key.clone());
        }
        if matches!(port.rate, PortRate::Kr | PortRate::Tr) {
            let mut hit = false;
            if state.param_routes_set.remove(&key).is_some() {
                hit = true;
            }
            if state.param_routes_bend.remove(&key).is_some() {
                hit = true;
            }
            if state.param_routes_trigger.remove(&key).is_some() {
                hit = true;
            }
            if hit {
                dropped_param_routes.push(key);
            }
        }
    }

    // ---- Allocate buses and default-route added ports --------------------
    //
    // Each added entry carries the *new* rate. Ar ports take a fresh chunk
    // from the audio-bus allocator; kr ports take a single control bus, then
    // we wrap it in a `BusId` to fit the voice's `output_buses` storage
    // shape (rate is re-resolved from the synthdef descriptor on teardown,
    // matching the create-time path in `handlers/voices.rs`).
    let mut default_muted = Vec::new();
    for port in &diff.added {
        let bus = match port.rate {
            PortRate::Ar => state.alloc_audio_bus(port.channels),
            PortRate::Kr | PortRate::Tr => BusId::new(state.alloc_control_bus().raw()),
        };
        if let Some(voice) = state.voices.get_mut(&voice_id) {
            voice.output_buses.push((port.name.clone(), bus));
        }
        // Story 5's silent-default rule applies to ar ports only — RouteMap
        // stores Group/Main/Muted dests, none of which are valid for a kr
        // port (kr destinations are Param routes in `state.param_routes`,
        // not RouteMap entries). Leaving a kr port out of `routes` is the
        // signal that "no Param route is installed yet"; the user's
        // `.to_param(...)` call is what later registers the dest.
        if port.name != "out" && matches!(port.rate, PortRate::Ar) {
            let key = (voice_id, port.name.clone());
            // Only inject the silent default if the script hasn't already
            // declared a route for the new port — preserves user intent.
            routes.entry(key.clone()).or_insert_with(|| vec![RouteDest::Muted]);
            default_muted.push(key);
        }
    }

    // Source of truth: overwrite the registered port set so future bus-free
    // lookups (e.g. on voice teardown) and the next reload diff see the new
    // shape.
    state.synthdef_outputs.insert(synthdef, new_ports.to_vec());

    // Emit a single warn line per rate-flipped port, naming the transition
    // and any routes that had to drop because of it. Pure name removals are
    // already covered by the caller's "dropped route" warn pass.
    let rate_changes = diff.rate_changes();
    for (name, old_rate, new_rate) in &rate_changes {
        let dropped: Vec<&str> = dropped_routes
            .iter()
            .chain(dropped_param_routes.iter())
            .filter(|(_, port_name)| port_name == name)
            .map(|(_, port_name)| port_name.as_str())
            .collect();
        if dropped.is_empty() {
            tracing::warn!(
                "port '{}' rate changed {}→{}",
                name,
                rate_label(*old_rate),
                rate_label(*new_rate),
            );
        } else {
            tracing::warn!(
                "port '{}' rate changed {}→{}; routes dropped: {:?}",
                name,
                rate_label(*old_rate),
                rate_label(*new_rate),
                dropped,
            );
        }
    }

    PortReconcile {
        diff,
        dropped_routes,
        dropped_param_routes,
        default_muted,
        rate_changes,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::state::{GroupState, VoiceState};
    use crate::traits::VoiceConfig;
    use crate::types::{GroupId, ParamMap};
    use std::collections::HashMap;

    fn port(name: &str, channels: u8) -> OutputPort {
        OutputPort {
            name: name.to_string(),
            channels,
            rate: PortRate::Ar,
        }
    }

    fn kr_port(name: &str, channels: u8) -> OutputPort {
        OutputPort {
            name: name.to_string(),
            channels,
            rate: PortRate::Kr,
        }
    }

    fn tr_port(name: &str, channels: u8) -> OutputPort {
        OutputPort {
            name: name.to_string(),
            channels,
            rate: PortRate::Tr,
        }
    }

    // -------------------------------------------------------------------
    // Pure diff
    // -------------------------------------------------------------------

    #[test]
    fn diff_unchanged_when_names_match() {
        let old = vec![port("out", 2), port("cv1", 1), port("cv2", 1), port("cv3", 1)];
        let new = old.clone();
        let d = diff_port_set(&old, &new);
        assert!(d.is_unchanged());
        assert_eq!(d.kept.len(), 4);
    }

    #[test]
    fn diff_unchanged_ignores_order() {
        let old = vec![port("a", 1), port("b", 1), port("c", 1)];
        let new = vec![port("c", 1), port("a", 1), port("b", 1)];
        let d = diff_port_set(&old, &new);
        assert!(
            d.is_unchanged(),
            "name-keyed diff must be order-insensitive"
        );
    }

    #[test]
    fn diff_added_only() {
        let old = vec![port("out", 2)];
        let new = vec![port("out", 2), port("cv1", 1)];
        let d = diff_port_set(&old, &new);
        assert_eq!(d.added, vec![port("cv1", 1)]);
        assert!(d.removed.is_empty());
    }

    #[test]
    fn diff_removed_only() {
        let old = vec![port("out", 2), port("cv1", 1)];
        let new = vec![port("out", 2)];
        let d = diff_port_set(&old, &new);
        assert!(d.added.is_empty());
        assert_eq!(d.removed, vec![port("cv1", 1)]);
    }

    #[test]
    fn diff_renamed_surfaces_as_remove_plus_add() {
        let old = vec![port("cv_old", 1)];
        let new = vec![port("cv_new", 1)];
        let d = diff_port_set(&old, &new);
        assert_eq!(d.added, vec![port("cv_new", 1)]);
        assert_eq!(d.removed, vec![port("cv_old", 1)]);
        assert!(d.kept.is_empty());
    }

    // -------------------------------------------------------------------
    // reconcile_voice_ports
    // -------------------------------------------------------------------

    /// Build a State with one voice that owns `ports` and registers `synth`
    /// in `state.synthdef_outputs`. Returns `(voice_id, group_id, synth)`.
    fn setup_voice_with_ports(
        state: &mut State,
        synth: &str,
        ports: &[OutputPort],
    ) -> (VoiceId, GroupId, String) {
        let voice_id = VoiceId::new(7);
        let group_id = GroupId::new(1);

        state.synthdefs.insert(synth.to_string());
        state
            .synthdef_outputs
            .insert(synth.to_string(), ports.to_vec());

        let group_node = state.alloc_node_id();
        let group_bus = state.alloc_audio_bus(2);
        state.groups.insert(
            group_id,
            GroupState {
                id: group_id,
                name: "g".to_string(),
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

        let mut output_buses = Vec::with_capacity(ports.len());
        for p in ports {
            let bus = match p.rate {
                PortRate::Ar => state.alloc_audio_bus(p.channels),
                PortRate::Kr | PortRate::Tr => BusId::new(state.alloc_control_bus().raw()),
            };
            output_buses.push((p.name.clone(), bus));
        }
        state.voices.insert(
            voice_id,
            VoiceState {
                id: voice_id,
                config: VoiceConfig::new("v", synth, group_id),
                active_nodes: Vec::new(),
                note_nodes: HashMap::new(),
                round_robin_position: 0,
                pending_params: HashMap::new(),
                output_buses,
            },
        );

        (voice_id, group_id, synth.to_string())
    }

    #[test]
    fn reconcile_body_only_edit_preserves_buses_and_routes() {
        // A 4-port synthdef whose body changed but ports stayed identical.
        // Every route entry must survive untouched, every bus assignment
        // must stay frozen — only the synth nodes (handled elsewhere)
        // recreate.
        let mut state = State::default();
        let ports = vec![port("out", 2), port("a", 1), port("b", 1), port("c", 1)];
        let (voice_id, group_id, _) =
            setup_voice_with_ports(&mut state, "voice_synth", &ports);

        let buses_before: Vec<_> = state.voices[&voice_id].output_buses.clone();
        let bus_count_before = state.audio_buses.allocated_count();

        let mut routes: RouteMap = HashMap::new();
        for p in &ports {
            routes.insert(
                (voice_id, p.name.clone()),
                vec![RouteDest::Group(group_id)],
            );
        }
        let routes_before = routes.clone();

        let outcome = reconcile_voice_ports(&mut state, voice_id, &ports, &mut routes);

        assert!(outcome.diff.is_unchanged());
        assert!(outcome.dropped_routes.is_empty());
        assert!(outcome.default_muted.is_empty());
        assert_eq!(state.voices[&voice_id].output_buses, buses_before);
        assert_eq!(state.audio_buses.allocated_count(), bus_count_before);
        assert_eq!(routes, routes_before, "route map left intact");
    }

    #[test]
    fn reconcile_added_port_allocates_bus_and_defaults_to_silent() {
        // A 5th port appears on a 4-port synthdef. The first 4 routes must
        // survive byte-for-byte; the new port must end up routable (bus
        // allocated) and silent (Muted, since its name is not "out").
        let mut state = State::default();
        let old = vec![port("out", 2), port("a", 1), port("b", 1), port("c", 1)];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let mut routes: RouteMap = HashMap::new();
        for p in &old {
            routes.insert(
                (voice_id, p.name.clone()),
                vec![RouteDest::Group(group_id)],
            );
        }

        let new = vec![
            port("out", 2),
            port("a", 1),
            port("b", 1),
            port("c", 1),
            port("d", 1),
        ];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.added, vec![port("d", 1)]);
        assert!(outcome.diff.removed.is_empty());
        assert!(outcome.dropped_routes.is_empty());
        assert_eq!(outcome.default_muted, vec![(voice_id, "d".to_string())]);

        // The 4 existing routes survive unchanged.
        for p in &old {
            assert_eq!(
                routes.get(&(voice_id, p.name.clone())),
                Some(&vec![RouteDest::Group(group_id)]),
                "existing route for {:?} disturbed",
                p.name
            );
        }
        // New port gets a silent default.
        assert_eq!(
            routes.get(&(voice_id, "d".to_string())),
            Some(&vec![RouteDest::Muted])
        );

        // A bus was allocated for the new port.
        let new_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "d")
            .map(|(_, b)| *b);
        assert!(new_bus.is_some(), "new port must have a bus chunk");

        // The registered port set now matches the new shape — subsequent
        // free_voice_output_buses / next-reload diff see the up-to-date set.
        assert_eq!(state.synthdef_outputs(&"vs".to_string()), new);
    }

    #[test]
    fn reconcile_added_out_port_does_not_inject_default_route() {
        // Edge case: a script adds a port literally named `out`. Story 5's
        // count rule treats `out` as user-managed, so we leave routing
        // entirely up to the script — no default-Muted entry.
        let mut state = State::default();
        let old = vec![port("cv", 1)];
        let (voice_id, _, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let mut routes: RouteMap = HashMap::new();
        let new = vec![port("cv", 1), port("out", 2)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.added, vec![port("out", 2)]);
        assert!(outcome.default_muted.is_empty());
        assert!(routes.is_empty(), "no default route for new `out` port");
    }

    #[test]
    fn reconcile_removed_port_frees_bus_drops_route_and_warns() {
        // Removing a routed port: its bus chunk goes back to the allocator,
        // the route entry vanishes from the map, and the dropped key is
        // returned to the caller for warning emission. Story 6b's
        // finalize will then free the orphan mixer synth via the
        // `removals` branch of the route diff.
        let mut state = State::default();
        let old = vec![port("out", 2), port("send", 1)];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let send_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "send")
            .map(|(_, b)| *b)
            .unwrap();

        let mut routes: RouteMap = HashMap::new();
        routes.insert((voice_id, "out".into()), vec![RouteDest::Group(group_id)]);
        routes.insert((voice_id, "send".into()), vec![RouteDest::Group(group_id)]);

        let new = vec![port("out", 2)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.removed, vec![port("send", 1)]);
        assert_eq!(
            outcome.dropped_routes,
            vec![(voice_id, "send".to_string())]
        );

        // Route gone — Story 6b's finalize will treat this as a removal.
        assert!(!routes.contains_key(&(voice_id, "send".to_string())));
        // Sibling route untouched.
        assert_eq!(
            routes.get(&(voice_id, "out".to_string())),
            Some(&vec![RouteDest::Group(group_id)])
        );

        // Bus chunk back in the allocator's free list.
        let reused = state.alloc_audio_bus(1);
        assert_eq!(
            reused, send_bus,
            "freed bus chunk should be the next allocation of matching width"
        );

        // Voice's output_buses no longer references the dropped port.
        assert!(!state.voices[&voice_id]
            .output_buses
            .iter()
            .any(|(n, _)| n == "send"));
    }

    #[test]
    fn reconcile_renamed_port_drops_old_route_adds_silent_new() {
        // Rename = remove + add. The old route is reported as dropped (so
        // the caller can warn the user to re-route under the new name) and
        // the new port comes up silent until explicitly routed.
        let mut state = State::default();
        let old = vec![port("cv_old", 1)];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let mut routes: RouteMap = HashMap::new();
        routes.insert((voice_id, "cv_old".into()), vec![RouteDest::Group(group_id)]);

        let new = vec![port("cv_new", 1)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.removed, vec![port("cv_old", 1)]);
        assert_eq!(outcome.diff.added, vec![port("cv_new", 1)]);
        assert_eq!(
            outcome.dropped_routes,
            vec![(voice_id, "cv_old".to_string())]
        );
        assert_eq!(
            outcome.default_muted,
            vec![(voice_id, "cv_new".to_string())]
        );

        assert!(!routes.contains_key(&(voice_id, "cv_old".to_string())));
        assert_eq!(
            routes.get(&(voice_id, "cv_new".to_string())),
            Some(&vec![RouteDest::Muted])
        );

        // Voice now owns a bus for cv_new and none for cv_old.
        let buses = &state.voices[&voice_id].output_buses;
        assert!(buses.iter().any(|(n, _)| n == "cv_new"));
        assert!(!buses.iter().any(|(n, _)| n == "cv_old"));
    }

    #[test]
    fn reconcile_preserves_user_route_for_added_port() {
        // If the script already declared a route for a newly added port,
        // we must not clobber it with the silent default — user intent
        // wins.
        let mut state = State::default();
        let old = vec![port("out", 2)];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let mut routes: RouteMap = HashMap::new();
        // User explicitly routes the new port to a group before reload runs.
        routes.insert((voice_id, "send".into()), vec![RouteDest::Group(group_id)]);

        let new = vec![port("out", 2), port("send", 1)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.added, vec![port("send", 1)]);
        assert_eq!(
            routes.get(&(voice_id, "send".to_string())),
            Some(&vec![RouteDest::Group(group_id)]),
            "user-declared route survived the reconcile"
        );
    }

    // -------------------------------------------------------------------
    // Story 8 — port-rate change diff (ar↔kr)
    // -------------------------------------------------------------------

    #[test]
    fn diff_rate_change_surfaces_as_remove_plus_add() {
        // Pure-diff sanity check: same name + flipped rate must split into
        // a removed (old-rate) and added (new-rate) pair, NOT survive in
        // `kept`. Otherwise the reconcile would never know to free the old
        // bus and allocate a new one from the matching pool.
        let old = vec![port("env", 1)];
        let new = vec![kr_port("env", 1)];
        let d = diff_port_set(&old, &new);
        assert_eq!(d.added, vec![kr_port("env", 1)]);
        assert_eq!(d.removed, vec![port("env", 1)]);
        assert!(d.kept.is_empty());
        assert_eq!(
            d.rate_changes(),
            vec![("env".to_string(), PortRate::Ar, PortRate::Kr)]
        );
    }

    #[test]
    fn diff_no_rate_change_when_rates_match() {
        // Same names, same rates, possibly different channel widths — must
        // stay in `kept` with an empty rate_changes list. Channel-width
        // changes are out of scope here (Story 6a).
        let old = vec![port("a", 1), kr_port("b", 1)];
        let new = vec![port("a", 2), kr_port("b", 1)]; // a's width ignored
        let d = diff_port_set(&old, &new);
        assert!(d.is_unchanged());
        assert!(d.rate_changes().is_empty());
    }

    #[test]
    fn reconcile_rate_change_ar_to_kr_drops_route_and_swaps_bus() {
        // 4-port synthdef with three ar ports. Edit "env" from ar to kr.
        // The existing Group route on "env" must drop with a rate-change
        // warning; sibling routes survive byte-for-byte; the old audio bus
        // returns to the audio-bus free list and a fresh control bus is
        // allocated for the kr "env".
        let mut state = State::default();
        let old = vec![
            port("out", 2),
            port("env", 1),
            port("a", 1),
            port("b", 1),
        ];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let env_old_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .unwrap();

        let mut routes: RouteMap = HashMap::new();
        for p in &old {
            routes.insert(
                (voice_id, p.name.clone()),
                vec![RouteDest::Group(group_id)],
            );
        }

        let new = vec![
            port("out", 2),
            kr_port("env", 1),
            port("a", 1),
            port("b", 1),
        ];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        // Diff: env appears in both removed (ar) and added (kr).
        assert_eq!(outcome.diff.removed, vec![port("env", 1)]);
        assert_eq!(outcome.diff.added, vec![kr_port("env", 1)]);
        assert_eq!(
            outcome.rate_changes,
            vec![("env".to_string(), PortRate::Ar, PortRate::Kr)]
        );

        // The old Group route on env got dropped; sibling routes intact.
        assert_eq!(
            outcome.dropped_routes,
            vec![(voice_id, "env".to_string())]
        );
        assert!(!routes.contains_key(&(voice_id, "env".to_string())));
        for sibling in &["out", "a", "b"] {
            assert_eq!(
                routes.get(&(voice_id, (*sibling).to_string())),
                Some(&vec![RouteDest::Group(group_id)]),
                "sibling route on {:?} disturbed by env rate flip",
                sibling
            );
        }

        // env's new bus came from the control-bus allocator (>= 1000 in
        // ControlBusAllocator's default range). Storage is a BusId so
        // compare on the raw range.
        let env_new_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .expect("env still owns a bus after rate flip");
        assert_ne!(env_new_bus, env_old_bus, "env must rebind to a new bus");
        assert!(
            env_new_bus.raw() >= 1000,
            "kr env must hold a control-bus id (got {})",
            env_new_bus.raw(),
        );

        // Old audio chunk back in the audio-bus free list — next mono alloc
        // returns it.
        let reused = state.alloc_audio_bus(1);
        assert_eq!(
            reused, env_old_bus,
            "freed audio chunk should be the next mono alloc"
        );

        // synthdef_outputs registry now reflects the kr env, so a future
        // teardown frees through the right pool.
        let registered = state.synthdef_outputs(&"vs".to_string());
        let env_in_registry = registered
            .iter()
            .find(|p| p.name == "env")
            .expect("env still registered");
        assert_eq!(env_in_registry.rate, PortRate::Kr);
    }

    #[test]
    fn reconcile_rate_change_kr_to_ar_drops_param_route_and_swaps_bus() {
        // Same shape as ar→kr but the other direction. The existing route on
        // "env" was a Param entry in state.param_routes (kr ports cannot use
        // Group/Main/Muted destinations). It must drop with a warning, the
        // control bus must return to the control-bus free list, and a fresh
        // audio bus must be allocated for the new ar env.
        let mut state = State::default();
        let old = vec![
            port("out", 2),
            kr_port("env", 1),
            port("a", 1),
            port("b", 1),
        ];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let env_old_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .unwrap();

        // Sibling Group routes for the ar ports — they stay in `routes`.
        let mut routes: RouteMap = HashMap::new();
        for p in &old {
            if matches!(p.rate, PortRate::Ar) {
                routes.insert(
                    (voice_id, p.name.clone()),
                    vec![RouteDest::Group(group_id)],
                );
            }
        }
        // env is kr, so its route lives in state.param_routes_set — pretend
        // the user installed a `to_param` mapping to some target.
        let target_voice = VoiceId::new(42);
        state.param_routes_set.insert(
            (voice_id, "env".to_string()),
            vec![(target_voice, "cutoff".to_string())],
        );

        let new = vec![
            port("out", 2),
            port("env", 1),
            port("a", 1),
            port("b", 1),
        ];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        // Diff: env appears in both removed (kr) and added (ar).
        assert_eq!(outcome.diff.removed, vec![kr_port("env", 1)]);
        assert_eq!(outcome.diff.added, vec![port("env", 1)]);
        assert_eq!(
            outcome.rate_changes,
            vec![("env".to_string(), PortRate::Kr, PortRate::Ar)]
        );

        // The Param route got drained from state.param_routes and surfaced
        // on the reconcile result. RouteMap was untouched for env (env had
        // no entry there to begin with).
        assert!(outcome.dropped_routes.is_empty());
        assert_eq!(
            outcome.dropped_param_routes,
            vec![(voice_id, "env".to_string())]
        );
        assert!(
            !state
                .param_routes_set
                .contains_key(&(voice_id, "env".to_string())),
            "kr→ar flip must drain the source-side Param route entry"
        );
        assert!(
            !state
                .param_routes_bend
                .contains_key(&(voice_id, "env".to_string())),
            "kr→ar flip must drain bend-side entries too",
        );
        // Sibling Group routes survive.
        for sibling in &["out", "a", "b"] {
            assert_eq!(
                routes.get(&(voice_id, (*sibling).to_string())),
                Some(&vec![RouteDest::Group(group_id)]),
                "sibling route on {:?} disturbed by env rate flip",
                sibling
            );
        }
        // env got a default Muted entry as a non-`out` newly-added ar port.
        assert_eq!(
            routes.get(&(voice_id, "env".to_string())),
            Some(&vec![RouteDest::Muted]),
            "ar env should default-route to silent per Story 5"
        );

        // env now holds an audio-bus id (under the control-bus floor of
        // 1000); the old control bus returned to the control-bus pool.
        let env_new_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .expect("env still owns a bus after rate flip");
        assert_ne!(env_new_bus, env_old_bus, "env must rebind to a new bus");
        assert!(
            env_new_bus.raw() < 1000,
            "ar env must hold an audio-bus id (got {})",
            env_new_bus.raw(),
        );

        // Next control-bus alloc reuses the freed env bus.
        let reused = state.alloc_control_bus();
        assert_eq!(
            reused.raw(),
            env_old_bus.raw(),
            "freed control bus should be the next alloc"
        );

        let registered = state.synthdef_outputs(&"vs".to_string());
        let env_in_registry = registered
            .iter()
            .find(|p| p.name == "env")
            .expect("env still registered");
        assert_eq!(env_in_registry.rate, PortRate::Ar);
    }

    // -------------------------------------------------------------------
    // B2.b — Tr port reconcile (rate change involving Tr is remove + add)
    // -------------------------------------------------------------------

    #[test]
    fn diff_kr_to_tr_surfaces_as_remove_plus_add() {
        // Same shape as the kr↔ar diff, but with a tr leg. Tr is a kr-rate
        // signal semantically, but downstream routing treats it as a distinct
        // destination shape (no scale/offset shaping), so the bus gets
        // re-allocated and dependent routes drop with a warning.
        let old = vec![kr_port("env", 1)];
        let new = vec![tr_port("env", 1)];
        let d = diff_port_set(&old, &new);
        assert_eq!(d.added, vec![tr_port("env", 1)]);
        assert_eq!(d.removed, vec![kr_port("env", 1)]);
        assert!(d.kept.is_empty());
        assert_eq!(
            d.rate_changes(),
            vec![("env".to_string(), PortRate::Kr, PortRate::Tr)]
        );
    }

    #[test]
    fn diff_ar_to_tr_surfaces_as_remove_plus_add() {
        let old = vec![port("env", 1)];
        let new = vec![tr_port("env", 1)];
        let d = diff_port_set(&old, &new);
        assert_eq!(d.added, vec![tr_port("env", 1)]);
        assert_eq!(d.removed, vec![port("env", 1)]);
        assert_eq!(
            d.rate_changes(),
            vec![("env".to_string(), PortRate::Ar, PortRate::Tr)]
        );
    }

    #[test]
    fn reconcile_voice_with_one_tr_port_owns_one_control_bus() {
        // Tr ports share the kr control-bus path. A 1-port tr synthdef must
        // carve exactly one ID from the control-bus pool (id ≥ 1000) and
        // leave the audio-bus counter untouched.
        let mut state = State::default();
        let ports = vec![tr_port("trig", 1)];
        let (voice_id, _, _) = setup_voice_with_ports(&mut state, "vs", &ports);

        let buses = &state.voices[&voice_id].output_buses;
        assert_eq!(buses.len(), 1, "one tr port → one output_bus entry");
        assert_eq!(buses[0].0, "trig");
        assert!(
            buses[0].1.raw() >= 1000,
            "tr port must own a control-bus id (got {})",
            buses[0].1.raw(),
        );
        assert_eq!(
            state.audio_buses.allocated_count(),
            2,
            "tr port does not advance the audio-bus counter past the group's stereo pair"
        );
        assert_eq!(
            state.control_buses.allocated_count(),
            1,
            "tr port advances the control-bus counter by 1"
        );
    }

    #[test]
    fn reconcile_rate_change_kr_to_tr_drops_param_route_and_swaps_bus() {
        // Kr → Tr is structurally remove + add, even though both ride the
        // control-bus path: the source port descriptor flips from kr to tr,
        // dependent routes drop with a warning (B2.c will re-route via
        // port_tr_to_param_link_1 once the user wires `.to_trigger`), and
        // the source bus gets a fresh control-bus id.
        let mut state = State::default();
        let old = vec![
            port("out", 2),
            kr_port("env", 1),
        ];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let env_old_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .unwrap();

        let mut routes: RouteMap = HashMap::new();
        routes.insert((voice_id, "out".into()), vec![RouteDest::Group(group_id)]);
        // Source-side Param route on the kr env port — must drain on flip.
        let target_voice = VoiceId::new(42);
        state.param_routes_set.insert(
            (voice_id, "env".to_string()),
            vec![(target_voice, "cutoff".to_string())],
        );

        let new = vec![
            port("out", 2),
            tr_port("env", 1),
        ];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.removed, vec![kr_port("env", 1)]);
        assert_eq!(outcome.diff.added, vec![tr_port("env", 1)]);
        assert_eq!(
            outcome.rate_changes,
            vec![("env".to_string(), PortRate::Kr, PortRate::Tr)]
        );

        // Tr ports also host source-side Param routes (B2.b), so the
        // teardown drained the kr→tr source key from the SET map.
        assert_eq!(
            outcome.dropped_param_routes,
            vec![(voice_id, "env".to_string())]
        );
        assert!(
            !state
                .param_routes_set
                .contains_key(&(voice_id, "env".to_string())),
            "kr→tr flip must drain the source-side Param route entry"
        );
        // Sibling Group route untouched.
        assert_eq!(
            routes.get(&(voice_id, "out".to_string())),
            Some(&vec![RouteDest::Group(group_id)])
        );
        // Tr ports do not get RouteMap default-Muted entries (RouteMap is
        // ar-only); the new tr port simply has no routing until the user
        // installs one via `.to_trigger` (B2.c).
        assert!(!routes.contains_key(&(voice_id, "env".to_string())));
        assert!(outcome.default_muted.is_empty());

        // env now holds a control-bus id. Kr and Tr share the same pool, so
        // the same id may come back from the free-list during the same
        // reconcile pass — what matters is that the bus is still in the
        // control range and the registry rate flipped to Tr.
        let env_new_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .expect("env still owns a bus after rate flip");
        assert!(
            env_new_bus.raw() >= 1000,
            "tr env must hold a control-bus id (got {})",
            env_new_bus.raw(),
        );
        // Bus counter does not advance — the freed kr id was reused.
        assert_eq!(
            state.control_buses.allocated_count(),
            1,
            "freed kr control-bus reused by the new tr port (counter unchanged)",
        );
        let _ = env_old_bus; // referenced for future-proof readers

        let registered = state.synthdef_outputs(&"vs".to_string());
        let env_in_registry = registered
            .iter()
            .find(|p| p.name == "env")
            .expect("env still registered");
        assert_eq!(env_in_registry.rate, PortRate::Tr);
    }

    #[test]
    fn reconcile_rate_change_ar_to_tr_drops_route_and_swaps_to_control_bus() {
        // Ar → Tr crosses the bus-allocator boundary: the audio chunk goes
        // back to the audio-bus free list, a fresh control-bus is carved,
        // and the existing Group route on the ar port drops with a warning.
        let mut state = State::default();
        let old = vec![port("env", 1), port("out", 2)];
        let (voice_id, group_id, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let env_old_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .unwrap();

        let mut routes: RouteMap = HashMap::new();
        routes.insert((voice_id, "env".into()), vec![RouteDest::Group(group_id)]);
        routes.insert((voice_id, "out".into()), vec![RouteDest::Group(group_id)]);

        let new = vec![tr_port("env", 1), port("out", 2)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.removed, vec![port("env", 1)]);
        assert_eq!(outcome.diff.added, vec![tr_port("env", 1)]);
        assert_eq!(
            outcome.rate_changes,
            vec![("env".to_string(), PortRate::Ar, PortRate::Tr)]
        );

        // The ar Group route on env got dropped; sibling out route survives.
        assert_eq!(
            outcome.dropped_routes,
            vec![(voice_id, "env".to_string())]
        );
        assert!(!routes.contains_key(&(voice_id, "env".to_string())));
        assert_eq!(
            routes.get(&(voice_id, "out".to_string())),
            Some(&vec![RouteDest::Group(group_id)])
        );

        // env now holds a control-bus id.
        let env_new_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "env")
            .map(|(_, b)| *b)
            .expect("env still owns a bus after rate flip");
        assert_ne!(env_new_bus, env_old_bus);
        assert!(
            env_new_bus.raw() >= 1000,
            "tr env must hold a control-bus id (got {})",
            env_new_bus.raw(),
        );

        // Old audio chunk is back in the audio-bus free list — next mono
        // alloc returns it.
        let reused = state.alloc_audio_bus(1);
        assert_eq!(reused, env_old_bus, "freed audio chunk reused");
    }

    #[test]
    fn reconcile_voice_drop_frees_tr_port_control_bus() {
        // The setup_voice_with_ports helper-bound voice owns a control bus
        // for its tr port; running the standard remove-port path
        // (replaying the synthdef without that port) should return the
        // control-bus id to the free-list pool.
        let mut state = State::default();
        let old = vec![tr_port("trig", 1), port("out", 2)];
        let (voice_id, _, _) = setup_voice_with_ports(&mut state, "vs", &old);

        let trig_bus = state.voices[&voice_id]
            .output_buses
            .iter()
            .find(|(n, _)| n == "trig")
            .map(|(_, b)| *b)
            .unwrap();
        assert!(trig_bus.raw() >= 1000);

        let mut routes: RouteMap = HashMap::new();
        let new = vec![port("out", 2)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.removed, vec![tr_port("trig", 1)]);
        assert!(!state.voices[&voice_id]
            .output_buses
            .iter()
            .any(|(n, _)| n == "trig"));

        // Bus back in the control-bus pool — next alloc reuses it.
        let reused = state.alloc_control_bus();
        assert_eq!(
            reused.raw(),
            trig_bus.raw(),
            "freed tr control-bus must be reused on next alloc",
        );
    }

    #[test]
    fn reconcile_no_diff_when_ports_unchanged_including_rates() {
        // Body-only edit on a mixed-rate synthdef: every port's name and
        // rate matches across versions, so reconcile is a no-op on buses,
        // routes, and the param_routes registry. rate_changes must be
        // empty too — a kept kr port is not a rate change.
        let mut state = State::default();
        let ports = vec![
            port("out", 2),
            port("a", 1),
            kr_port("env", 1),
            kr_port("lfo", 1),
        ];
        let (voice_id, group_id, _) =
            setup_voice_with_ports(&mut state, "vs", &ports);

        let buses_before = state.voices[&voice_id].output_buses.clone();
        let mut routes: RouteMap = HashMap::new();
        for p in &ports {
            if matches!(p.rate, PortRate::Ar) {
                routes.insert(
                    (voice_id, p.name.clone()),
                    vec![RouteDest::Group(group_id)],
                );
            }
        }
        let routes_before = routes.clone();
        state.param_routes_set.insert(
            (voice_id, "env".to_string()),
            vec![(VoiceId::new(99), "cutoff".to_string())],
        );

        let outcome = reconcile_voice_ports(&mut state, voice_id, &ports, &mut routes);

        assert!(outcome.diff.is_unchanged());
        assert!(outcome.dropped_routes.is_empty());
        assert!(outcome.dropped_param_routes.is_empty());
        assert!(outcome.default_muted.is_empty());
        assert!(outcome.rate_changes.is_empty());
        assert_eq!(state.voices[&voice_id].output_buses, buses_before);
        assert_eq!(routes, routes_before);
        assert!(state
            .param_routes_set
            .contains_key(&(voice_id, "env".to_string())));
    }
}
