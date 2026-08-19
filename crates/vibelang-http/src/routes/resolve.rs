//! One identifier-resolution rule for every entity in the HTTP API.
//!
//! Route handlers take `{id}` path segments that may be either a numeric id or
//! the script name the entity was declared under. Before this module each
//! route module spelled that out for itself, and they disagreed on all three
//! of the decisions involved: whether a name was accepted at all, which of
//! name and number was tried first, and whether an unknown identifier was a
//! 404 or a 400.
//!
//! [`define_id_resolver!`] generates one resolver per entity so the rule is
//! written once:
//!
//! 1. A numeric identifier that addresses a **live** entity wins. Liveness is
//!    what makes the numeric branch safe to try first — an entity legitimately
//!    declared as `fx("123")` is not shadowed by it, because id `123` only
//!    wins if some entity really carries it.
//! 2. Otherwise the identifier is matched against the **stored** name.
//! 3. Otherwise `404 Not Found`, naming the entity and the identifier.
//!
//! Names are matched, never re-derived. Ids are seeded from
//! `hash_name_to_id`, but `define_id_accessors!` probes forward while a slot
//! is occupied, so the hash is not invertible and recomputing it can resolve a
//! name onto an unrelated entity.

/// Generate a `resolve_*_id` function with the uniform rule above.
///
/// `$name_of` extracts the declared name from the entity's state value.
macro_rules! define_id_resolver {
    (
        $(#[$meta:meta])*
        $fn_name:ident,
        $lookup_name:ident,
        $id_ty:ty,
        $map:ident,
        $label:literal,
        |$value:ident| $name_of:expr
    ) => {
        /// Pure form of the resolution rule, over a borrowed [`State`].
        ///
        /// Split out so the rule is unit-testable without standing up an
        /// `AppState` (which needs a live runtime handle), and so the async
        /// wrapper takes the state read lock exactly once.
        #[allow(dead_code)]
        pub(crate) fn $lookup_name(
            state: &vibelang_core::State,
            identifier: &str,
        ) -> Option<$id_ty> {
            if let Ok(numeric) = identifier.parse::<u32>() {
                let candidate = <$id_ty>::new(numeric);
                if state.$map.contains_key(&candidate) {
                    return Some(candidate);
                }
                // Fall through: a name that merely looks numeric is still a name.
            }
            state
                .$map
                .iter()
                .find(|(_, $value)| $name_of == identifier)
                .map(|(id, _)| *id)
        }

        $(#[$meta])*
        #[allow(dead_code)]
        pub(crate) async fn $fn_name(
            state: &std::sync::Arc<$crate::AppState>,
            identifier: &str,
        ) -> Result<$id_ty, (axum::http::StatusCode, axum::Json<$crate::models::ErrorResponse>)> {
            match state.with_state(|s| $lookup_name(s, identifier)).await {
                Some(id) => Ok(id),
                None => Err((
                    axum::http::StatusCode::NOT_FOUND,
                    axum::Json($crate::models::ErrorResponse::not_found(&format!(
                        "{} '{}' not found",
                        $label, identifier
                    ))),
                )),
            }
        }
    };
}

pub(crate) use define_id_resolver;

define_id_resolver!(
    /// Resolve a voice by numeric id or declared name.
    resolve_voice_id,
    find_voice,
    vibelang_core::VoiceId,
    voices,
    "Voice",
    |v| v.config.name
);

define_id_resolver!(
    /// Resolve a group by numeric id or declared name.
    resolve_group_id,
    find_group,
    vibelang_core::GroupId,
    groups,
    "Group",
    |g| g.name
);

define_id_resolver!(
    /// Resolve a pattern by numeric id or declared name.
    resolve_pattern_id,
    find_pattern,
    vibelang_core::PatternId,
    patterns,
    "Pattern",
    |p| p.content.name
);

define_id_resolver!(
    /// Resolve a melody by numeric id or declared name.
    resolve_melody_id,
    find_melody,
    vibelang_core::MelodyId,
    melodies,
    "Melody",
    |m| m.content.name
);

define_id_resolver!(
    /// Resolve a sequence by numeric id or declared name.
    resolve_sequence_id,
    find_sequence,
    vibelang_core::SequenceId,
    sequences,
    "Sequence",
    |s| s.config.name
);

define_id_resolver!(
    /// Resolve an effect by numeric id or declared name.
    resolve_effect_id,
    find_effect,
    vibelang_core::EffectId,
    effects,
    "Effect",
    |e| e.name
);

#[cfg(test)]
mod tests {
    use super::{find_effect, find_group, find_voice};
    use vibelang_core::{
        hash_name_to_id, BusId, EffectId, EffectState, GroupId, GroupState, NodeId, ParamMap,
        State, VoiceId, VoiceState,
    };

    fn add_effect(state: &mut State, id: EffectId, name: &str) {
        state.effects.insert(
            id,
            EffectState {
                id,
                name: name.to_string(),
                group: GroupId::new(1),
                synthdef: "reverb".to_string(),
                node_id: NodeId::new(1000 + id.0),
                audio_bus: BusId::new(16),
                params: ParamMap::new(),
            },
        );
    }

    #[test]
    fn resolves_by_declared_name() {
        let mut state = State::default();
        let id = EffectId::new(hash_name_to_id("pad_filter"));
        add_effect(&mut state, id, "pad_filter");

        assert_eq!(find_effect(&state, "pad_filter"), Some(id));
        assert_eq!(find_effect(&state, "never_declared"), None);
    }

    #[test]
    fn resolves_by_numeric_id_when_live() {
        let mut state = State::default();
        let id = EffectId::new(4242);
        add_effect(&mut state, id, "verb");

        assert_eq!(find_effect(&state, "4242"), Some(id));
    }

    /// A numeric-looking *name* is still a name.
    ///
    /// The numeric branch only wins when it addresses a live entity, so
    /// `fx("123")` stays reachable by its own name even though `123` parses.
    #[test]
    fn numeric_looking_name_resolves_when_no_entity_carries_that_id() {
        let mut state = State::default();
        let id = EffectId::new(hash_name_to_id("123"));
        add_effect(&mut state, id, "123");

        assert_eq!(find_effect(&state, "123"), Some(id));
    }

    /// A live numeric id wins over a same-spelled name on a different entity.
    #[test]
    fn live_numeric_id_takes_precedence_over_numeric_looking_name() {
        let mut state = State::default();
        let squatter = EffectId::new(77);
        add_effect(&mut state, squatter, "unrelated");
        let named_77 = EffectId::new(hash_name_to_id("77"));
        add_effect(&mut state, named_77, "77");

        assert_eq!(find_effect(&state, "77"), Some(squatter));
    }

    /// The regression the name lookup exists for.
    ///
    /// `define_id_accessors!` seeds an id from `hash_name_to_id` but probes
    /// forward while a slot is occupied, so a probed entity does NOT live at
    /// the hash of its own name. Recomputing the hash — what the effects
    /// route did before — resolves such a name onto the unrelated entity
    /// squatting the slot, silently addressing the wrong node.
    #[test]
    fn probed_entity_resolves_to_itself_not_the_slot_squatter() {
        let squatter_id = EffectId::new(hash_name_to_id("squatter"));
        let probed_id = EffectId::new(squatter_id.0.wrapping_add(1));

        let mut state = State::default();
        add_effect(&mut state, squatter_id, "squatter");
        add_effect(&mut state, probed_id, "probed");

        assert_eq!(find_effect(&state, "squatter"), Some(squatter_id));
        assert_eq!(find_effect(&state, "probed"), Some(probed_id));
        assert_ne!(find_effect(&state, "probed"), Some(squatter_id));
    }

    /// The same rule, generated for a different entity and a different
    /// name accessor (`GroupState::name`, `VoiceState::config.name`).
    #[test]
    fn rule_is_identical_across_entities() {
        let mut state = State::default();

        let gid = GroupId::new(hash_name_to_id("drums"));
        state.groups.insert(
            gid,
            GroupState {
                id: gid,
                name: "drums".to_string(),
                parent: None,
                node_id: NodeId::new(1),
                audio_bus: BusId::new(16),
                link_synth_node_id: None,
                muted: false,
                soloed: false,
                params: ParamMap::new(),
                output_bus: None,
                output_channels: None,
            },
        );

        assert_eq!(find_group(&state, "drums"), Some(gid));
        assert_eq!(find_group(&state, "missing"), None);

        assert_eq!(find_voice(&state, "nothing_here"), None);
        let _ = VoiceId::new(1);
    }
}
