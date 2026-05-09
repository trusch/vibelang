//! Story 4 regression test: deterministic root-group allocation order.
//!
//! Background: today's bus assignment was randomised by `HashMap`/`HashSet`
//! iteration order — every reboot of the studio script flipped which root
//! group (`master` vs `es-3`) landed at which audio bus, combined with the
//! Story 1 bus-16 collision this is why "drums sometimes worked, sometimes
//! didn't" across reboots.
//!
//! The fix: `reload::order_group_creations` (and the symmetric
//! `order_group_deletions`) now sort each Kahn batch by `GroupId::raw()`
//! before extending the ordered output. `GroupId::raw()` is a stable
//! FNV-1a hash of the group path, so the resulting order is content-defined
//! across processes.
//!
//! What this test pins down: build the same logical group set repeatedly,
//! each time in a fresh `HashMap` (i.e. fresh `RandomState` seed), and
//! assert `order_group_creations` returns byte-identical output every run.
//! Without the per-batch sort this fails roughly half the time — Rust's
//! default hasher is randomised per-map, so a single in-process `HashMap`
//! has stable iteration but two distinct maps with the same contents do
//! not.

use std::collections::HashMap;

use vibelang_core::reload::{order_group_creations, GroupConfig};
use vibelang_core::types::GroupId;

fn root_config(name: &str) -> GroupConfig {
    GroupConfig {
        name: name.to_string(),
        parent: None,
        ..GroupConfig::default()
    }
}

fn child_config(name: &str, parent: GroupId) -> GroupConfig {
    GroupConfig {
        name: name.to_string(),
        parent: Some(parent),
        ..GroupConfig::default()
    }
}

/// Build the canonical 3-root scenario from the ticket: root groups `a`, `b`,
/// `c` (no parents). The bug surfaces here because all three qualify in the
/// first Kahn batch, so their order is whatever the `HashSet<GroupId>` in
/// `order_group_creations` chooses to iterate.
fn three_roots() -> HashMap<GroupId, GroupConfig> {
    let mut configs = HashMap::new();
    configs.insert(GroupId::new(1), root_config("a"));
    configs.insert(GroupId::new(2), root_config("b"));
    configs.insert(GroupId::new(3), root_config("c"));
    configs
}

#[test]
fn order_group_creations_is_deterministic_across_fresh_hashmaps() {
    // Each iteration constructs a brand-new HashMap, which gets a fresh
    // RandomState. Without the in-batch sort, two of these calls roughly
    // half the time will return `[a, c, b]` instead of `[a, b, c]`.
    let baseline = order_group_creations(&three_roots());
    for run in 0..100 {
        let configs = three_roots();
        let ordered = order_group_creations(&configs);
        assert_eq!(
            ordered, baseline,
            "run {} produced different group order than baseline — \
             order_group_creations is non-deterministic",
            run
        );
    }
}

#[test]
fn order_group_creations_sorts_roots_by_raw_id() {
    // Insert in an order that contradicts id.raw() to make sure we're
    // not relying on insertion order. Also use non-contiguous, non-monotonic
    // raw IDs (path FNV hashes won't be sequential in real scripts).
    let mut configs = HashMap::new();
    configs.insert(GroupId::new(0xDEAD_BEEF), root_config("z"));
    configs.insert(GroupId::new(0x0000_0042), root_config("a"));
    configs.insert(GroupId::new(0x1000_0000), root_config("m"));

    let ordered = order_group_creations(&configs);
    assert_eq!(
        ordered,
        vec![
            GroupId::new(0x0000_0042),
            GroupId::new(0x1000_0000),
            GroupId::new(0xDEAD_BEEF),
        ],
        "roots should be ordered by GroupId::raw() ascending within their batch"
    );
}

#[test]
fn order_group_creations_preserves_parent_before_child_with_sorted_ties() {
    // Two-tier hierarchy:
    //   roots: 100 ("alpha"), 5 ("beta")
    //   children of 100: 200, 50
    //   children of 5: 7
    // Expected: roots first sorted by raw -> [5, 100], then their children
    // sorted by raw within each batch.
    let mut configs = HashMap::new();
    configs.insert(GroupId::new(100), root_config("alpha"));
    configs.insert(GroupId::new(5), root_config("beta"));
    configs.insert(GroupId::new(200), child_config("alpha-1", GroupId::new(100)));
    configs.insert(GroupId::new(50), child_config("alpha-2", GroupId::new(100)));
    configs.insert(GroupId::new(7), child_config("beta-1", GroupId::new(5)));

    let ordered = order_group_creations(&configs);

    // Parents must precede children.
    let pos = |id: GroupId| ordered.iter().position(|&x| x == id).unwrap();
    assert!(pos(GroupId::new(5)) < pos(GroupId::new(7)));
    assert!(pos(GroupId::new(100)) < pos(GroupId::new(50)));
    assert!(pos(GroupId::new(100)) < pos(GroupId::new(200)));

    // First batch (roots) sorted ascending by raw().
    assert!(pos(GroupId::new(5)) < pos(GroupId::new(100)));

    // Second batch (children of those roots) sorted ascending by raw()
    // within the batch.
    assert!(pos(GroupId::new(7)) < pos(GroupId::new(50)));
    assert!(pos(GroupId::new(50)) < pos(GroupId::new(200)));

    // And the absolute order is fully determined.
    assert_eq!(
        ordered,
        vec![
            GroupId::new(5),
            GroupId::new(100),
            GroupId::new(7),
            GroupId::new(50),
            GroupId::new(200),
        ]
    );
}

#[test]
fn order_group_creations_stable_under_repeated_calls_same_map() {
    // Within a single map (single RandomState), iteration is already stable
    // in practice — but the sort means we don't rely on that detail.
    let configs = three_roots();
    let first = order_group_creations(&configs);
    for _ in 0..50 {
        assert_eq!(order_group_creations(&configs), first);
    }
}
