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
use crate::types::VoiceId;
use std::collections::HashMap;
use vibelang_dsp::OutputPort;

/// Diff between two port sets, keyed by port name.
///
/// Order-insensitive — only the set of names matters. A rename surfaces as a
/// pair of (`removed`, `added`) entries; a body-only edit leaves both lists
/// empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortSetDiff {
    /// Ports present in `new` but not in `old`.
    pub added: Vec<OutputPort>,
    /// Ports present in `old` but not in `new`. Carries the channel count so
    /// the matching bus chunk can be returned to the allocator.
    pub removed: Vec<OutputPort>,
    /// Ports present under the same name in both. Carries the *new* channel
    /// count; a channel-count change on a kept name is out of scope here
    /// (Story 6a treats it as the same routable destination).
    pub kept: Vec<OutputPort>,
}

impl PortSetDiff {
    /// True when the port set is identical (nothing added or removed).
    pub fn is_unchanged(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Compute the port-set diff, name-keyed and order-insensitive.
pub fn diff_port_set(old: &[OutputPort], new: &[OutputPort]) -> PortSetDiff {
    let old_by_name: HashMap<&str, &OutputPort> =
        old.iter().map(|p| (p.name.as_str(), p)).collect();
    let new_by_name: HashMap<&str, &OutputPort> =
        new.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut added = Vec::new();
    let mut kept = Vec::new();
    for p in new {
        if old_by_name.contains_key(p.name.as_str()) {
            kept.push(p.clone());
        } else {
            added.push(p.clone());
        }
    }

    let mut removed = Vec::new();
    for p in old {
        if !new_by_name.contains_key(p.name.as_str()) {
            removed.push(p.clone());
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
/// exists. The caller is expected to log a warning naming each dropped route
/// so the user can clean up their script.
#[derive(Clone, Debug, Default)]
pub struct PortReconcile {
    /// The computed port-set diff (added/removed/kept).
    pub diff: PortSetDiff,
    /// Route keys removed from the route map because their port disappeared.
    pub dropped_routes: Vec<(VoiceId, String)>,
    /// Route keys inserted with `RouteDest::Muted` for newly added non-`out`
    /// ports per Story 5's silent-by-default rule.
    pub default_muted: Vec<(VoiceId, String)>,
}

/// Reconcile a voice's bus allocations and route entries to match `new_ports`.
///
/// Mutates:
/// - `state.voices[voice_id].output_buses` — drops removed ports and appends
///   newly allocated ones.
/// - The audio bus allocator (via [`State::free_audio_bus`] /
///   [`State::alloc_audio_bus`]).
/// - `state.synthdef_outputs[voice.synthdef]` — overwritten with the new
///   set so subsequent [`State::synthdef_outputs`](crate::state::State::synthdef_outputs)
///   lookups see the up-to-date shape.
/// - `routes` — entries whose port name is in `diff.removed` are dropped;
///   newly added non-`out` ports get a default `RouteDest::Muted` entry so
///   the route diff against `current_routes` will spawn no mixer synth.
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
    let mut dropped_routes = Vec::new();
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
            state.free_audio_bus(bus, port.channels);
        }
        if let Some(voice) = state.voices.get_mut(&voice_id) {
            voice.output_buses.retain(|(n, _)| n != &port.name);
        }
        let key = (voice_id, port.name.clone());
        if routes.remove(&key).is_some() {
            dropped_routes.push(key);
        }
    }

    // ---- Allocate buses and default-route added ports --------------------
    let mut default_muted = Vec::new();
    for port in &diff.added {
        let bus = state.alloc_audio_bus(port.channels);
        if let Some(voice) = state.voices.get_mut(&voice_id) {
            voice.output_buses.push((port.name.clone(), bus));
        }
        if port.name != "out" {
            let key = (voice_id, port.name.clone());
            // Only inject the silent default if the script hasn't already
            // declared a route for the new port — preserves user intent.
            routes.entry(key.clone()).or_insert(RouteDest::Muted);
            default_muted.push(key);
        }
    }

    // Source of truth: overwrite the registered port set so future bus-free
    // lookups (e.g. on voice teardown) and the next reload diff see the new
    // shape.
    state.synthdef_outputs.insert(synthdef, new_ports.to_vec());

    PortReconcile {
        diff,
        dropped_routes,
        default_muted,
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
            },
        );

        let mut output_buses = Vec::with_capacity(ports.len());
        for p in ports {
            output_buses.push((p.name.clone(), state.alloc_audio_bus(p.channels)));
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
                RouteDest::Group(group_id),
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
                RouteDest::Group(group_id),
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
                Some(&RouteDest::Group(group_id)),
                "existing route for {:?} disturbed",
                p.name
            );
        }
        // New port gets a silent default.
        assert_eq!(
            routes.get(&(voice_id, "d".to_string())),
            Some(&RouteDest::Muted)
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
        routes.insert((voice_id, "out".into()), RouteDest::Group(group_id));
        routes.insert((voice_id, "send".into()), RouteDest::Group(group_id));

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
            Some(&RouteDest::Group(group_id))
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
        routes.insert((voice_id, "cv_old".into()), RouteDest::Group(group_id));

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
            Some(&RouteDest::Muted)
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
        routes.insert((voice_id, "send".into()), RouteDest::Group(group_id));

        let new = vec![port("out", 2), port("send", 1)];
        let outcome = reconcile_voice_ports(&mut state, voice_id, &new, &mut routes);

        assert_eq!(outcome.diff.added, vec![port("send", 1)]);
        assert_eq!(
            routes.get(&(voice_id, "send".to_string())),
            Some(&RouteDest::Group(group_id)),
            "user-declared route survived the reconcile"
        );
    }
}
