//! A reload must never leave a child group's link synth AFTER the node that
//! reads its parent's bus.
//!
//! Board defect (super-gamma, 2026-08-17): after a run of preset recalls a
//! reload left `system_link_audio` nodes writing the parent's audio bus while
//! the parent's own link synth — the only reader of that bus — had already
//! run earlier in the same cycle. scsynth zeroes audio buses every block, so
//! the whole performance side went silent with every SynthDef loaded and
//! every synth alive. A recall could not repair it; only restarting the
//! engine could.
//!
//! The mock backend simulates the scsynth node tree (ordered children,
//! honouring AddActions and `/n_before`), and records the `inbus`/`outbus`
//! controls of every synth, so the assertion is the ENGINE'S OWN verdict:
//! `audio_path_breaks` over the live tree in evaluation order.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use vibelang_core::backends::scsynth::{audio_path_breaks, AudioPathBreak, TreeSynth};
use vibelang_core::reload::{EffectConfig, GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, EffectId, GroupId, NodeId, ParamMap, ReloadMessage,
    Runtime, SynthDefMessage,
};

// =========================================================================
// Mock backend with a node-tree simulation.
// =========================================================================

#[derive(Debug)]
struct MockError(String);

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error: {}", self.0)
    }
}

impl std::error::Error for MockError {}

/// Minimal scsynth tree: ordered children per group, honouring AddActions
/// and `/n_before` moves. Root group is NodeId(0).
#[derive(Default)]
struct Tree {
    children: HashMap<NodeId, Vec<NodeId>>,
    parent: HashMap<NodeId, NodeId>,
    defs: HashMap<NodeId, String>,
    controls: HashMap<NodeId, ParamMap>,
}

impl Tree {
    fn insert(&mut self, node: NodeId, target: NodeId, action: AddAction) -> Result<(), MockError> {
        self.children.entry(target).or_default();
        match action {
            AddAction::Head => {
                self.children.get_mut(&target).unwrap().insert(0, node);
                self.parent.insert(node, target);
            }
            AddAction::Tail => {
                self.children.get_mut(&target).unwrap().push(node);
                self.parent.insert(node, target);
            }
            AddAction::Before | AddAction::After => {
                let parent = *self
                    .parent
                    .get(&target)
                    .ok_or_else(|| MockError(format!("{:?} has no parent", target)))?;
                let siblings = self.children.get_mut(&parent).unwrap();
                let idx = siblings
                    .iter()
                    .position(|n| *n == target)
                    .ok_or_else(|| MockError(format!("{:?} not among siblings", target)))?;
                let idx = if action == AddAction::After {
                    idx + 1
                } else {
                    idx
                };
                siblings.insert(idx, node);
                self.parent.insert(node, parent);
            }
            AddAction::Replace => return Err(MockError("Replace unsupported".into())),
        }
        self.children.entry(node).or_default();
        Ok(())
    }

    fn free(&mut self, node: NodeId) {
        if let Some(parent) = self.parent.remove(&node) {
            if let Some(siblings) = self.children.get_mut(&parent) {
                siblings.retain(|n| *n != node);
            }
        }
        self.defs.remove(&node);
        self.controls.remove(&node);
        if let Some(kids) = self.children.remove(&node) {
            for kid in kids {
                self.free(kid);
            }
        }
    }

    fn move_before(&mut self, node: NodeId, before: NodeId) -> Result<(), MockError> {
        let old_parent = self
            .parent
            .remove(&node)
            .ok_or_else(|| MockError(format!("moved node {:?} not in tree", node)))?;
        self.children
            .get_mut(&old_parent)
            .unwrap()
            .retain(|n| *n != node);
        let new_parent = *self
            .parent
            .get(&before)
            .ok_or_else(|| MockError(format!("anchor {:?} not in tree", before)))?;
        let siblings = self.children.get_mut(&new_parent).unwrap();
        let idx = siblings
            .iter()
            .position(|n| *n == before)
            .ok_or_else(|| MockError(format!("anchor {:?} not among siblings", before)))?;
        siblings.insert(idx, node);
        self.parent.insert(node, new_parent);
        Ok(())
    }

    /// Every synth under `root` in evaluation order — exactly what
    /// `/g_queryTree` reports and what `audio_path_breaks` expects.
    fn synths_in_evaluation_order(&self, root: NodeId) -> Vec<TreeSynth> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self
            .children
            .get(&root)
            .map(|kids| kids.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(node) = stack.pop() {
            if let Some(def) = self.defs.get(&node) {
                out.push(TreeSynth {
                    node: node.0 as i32,
                    def: def.clone(),
                    controls: self
                        .controls
                        .get(&node)
                        .map(|p| p.iter().map(|(k, v)| (k.clone(), *v)).collect())
                        .unwrap_or_default(),
                });
            }
            if let Some(kids) = self.children.get(&node) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        out
    }
}

#[derive(Default)]
struct BackendState {
    tree: Mutex<Tree>,
}

#[derive(Clone, Default)]
struct MockBackend {
    state: Arc<BackendState>,
}

impl MockBackend {
    fn new() -> Self {
        Self::default()
    }

    /// The live tree as the engine's own graph-integrity check sees it.
    fn tree(&self) -> Vec<TreeSynth> {
        self.state
            .tree
            .lock()
            .unwrap()
            .synths_in_evaluation_order(NodeId::new(0))
    }

    fn breaks(&self) -> Vec<AudioPathBreak> {
        audio_path_breaks(&self.tree())
    }

    /// The whole tree — groups included — in evaluation order, so a failing
    /// assertion says WHERE a node landed and not just that it is stranded.
    fn render(&self) -> String {
        let tree = self.state.tree.lock().unwrap();
        let mut out = String::new();
        let mut stack: Vec<(NodeId, usize)> = tree
            .children
            .get(&NodeId::new(0))
            .map(|kids| kids.iter().rev().map(|n| (*n, 1usize)).collect())
            .unwrap_or_default();
        while let Some((node, depth)) = stack.pop() {
            let pad = "  ".repeat(depth);
            match tree.defs.get(&node) {
                Some(def) => {
                    let ctl = tree.controls.get(&node);
                    let get = |k: &str| ctl.and_then(|c| c.get(k)).copied();
                    let ports = match (get("inbus"), get("outbus")) {
                        (Some(i), Some(o)) => format!(" in={i} out={o}"),
                        (None, Some(o)) => format!(" out={o}"),
                        (Some(i), None) => format!(" in={i}"),
                        (None, None) => String::new(),
                    };
                    out.push_str(&format!("{pad}{} {}{}\n", node.0, def, ports));
                }
                None => out.push_str(&format!("{pad}{} [group]\n", node.0)),
            }
            if let Some(kids) = tree.children.get(&node) {
                stack.extend(kids.iter().rev().map(|n| (*n, depth + 1)));
            }
        }
        out
    }
}

#[async_trait]
impl Backend for MockBackend {
    type Error = MockError;

    async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> Result<(), Self::Error> {
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
        let mut tree = self.state.tree.lock().unwrap();
        tree.insert(node, target, action)?;
        tree.defs.insert(node, def.to_string());
        tree.controls.insert(node, params.clone());
        Ok(())
    }

    async fn create_group(
        &self,
        node: NodeId,
        target: NodeId,
        action: AddAction,
    ) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().insert(node, target, action)
    }

    async fn free_node(&self, node: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().free(node);
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn move_node_before(&self, node: NodeId, before: NodeId) -> Result<(), Self::Error> {
        self.state.tree.lock().unwrap().move_before(node, before)
    }

    async fn set_param(&self, node: NodeId, param: &str, value: f32) -> Result<(), Self::Error> {
        let mut tree = self.state.tree.lock().unwrap();
        if let Some(controls) = tree.controls.get_mut(&node) {
            controls.insert(param.to_string(), value);
        }
        Ok(())
    }

    async fn map_param_to_bus(
        &self,
        _node: NodeId,
        _param: &str,
        _bus: u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn load_buffer(&self, _id: BufferId, _path: &Path) -> Result<BufferInfo, Self::Error> {
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
    ) -> Result<BufferInfo, Self::Error> {
        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: 0.0,
        })
    }

    async fn write_buffer(&self, _id: BufferId, _path: &Path) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_buffer(&self, _id: BufferId) -> Result<(), Self::Error> {
        Ok(())
    }

    fn current_time(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

// =========================================================================
// Fixtures — the shape of a super-gamma patch.
// =========================================================================

/// Group and effect ids on the board are FNV hashes of the entity's name,
/// so the SAME id comes back patch after patch — sometimes under a
/// different master, sometimes not at all. These fixtures keep that: a
/// small pool of ids, recombined per patch.
const MASTER_A: GroupId = GroupId(1);
const MASTER_B: GroupId = GroupId(2);
const SIDE_A: GroupId = GroupId(3);
const SIDE_B: GroupId = GroupId(4);
const SIDE_C: GroupId = GroupId(5);

/// The board's factory patches carry the same effect *names* — hence the
/// same ids — on the master chain of one patch and on a performance side of
/// the next. That migration is the "synthdef or group changed" recreate.
const FX_1: EffectId = EffectId(10);
const FX_2: EffectId = EffectId(11);
const FX_3: EffectId = EffectId(12);

const DEF_FX: &str = "chain_fx";

/// One performance side: the group plus the effects on it.
struct Side {
    group: GroupId,
    fx: &'static [EffectId],
}

/// A patch: one master group carrying a serial fx chain, plus one
/// performance-side sub-group per entry in `sides` — the
/// two-sides-into-a-master-chain shape of every factory patch.
fn patch(master: GroupId, master_fx: &[EffectId], sides: &[Side]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        master,
        GroupConfig {
            name: format!("master{}", master.0),
            effects: master_fx.to_vec(),
            ..Default::default()
        },
    );
    for id in master_fx {
        script.add_effect(
            *id,
            EffectConfig {
                group: master,
                synthdef: DEF_FX.to_string(),
                params: ParamMap::new(),
            },
        );
    }
    for side in sides {
        script.add_group(
            side.group,
            GroupConfig {
                name: format!("master{}/side{}", master.0, side.group.0),
                parent: Some(master),
                effects: side.fx.to_vec(),
                ..Default::default()
            },
        );
        for id in side.fx {
            script.add_effect(
                *id,
                EffectConfig {
                    group: side.group,
                    synthdef: DEF_FX.to_string(),
                    params: ParamMap::new(),
                },
            );
        }
    }
    script
}

/// The board shape, exactly: one master that SURVIVES the reload under a
/// constant id, carrying a serial fx chain whose synthdef CHANGES, plus one
/// child group whose id CHANGES because ids hash the full path.
///
/// `master_fx` is given in audio-path order — the order the chain runs in.
/// It is deliberately NOT id order in the tests below, so that the
/// `min_by_key(id.raw())` fallback in `State::first_effect_node_in_group`
/// picks a different node than the true chain head.
fn patch_with_def(
    master: GroupId,
    master_fx: &[EffectId],
    master_def: &str,
    sides: &[(GroupId, &[EffectId], &str)],
) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        master,
        GroupConfig {
            name: format!("master{}", master.0),
            effects: master_fx.to_vec(),
            ..Default::default()
        },
    );
    for id in master_fx {
        script.add_effect(
            *id,
            EffectConfig {
                group: master,
                synthdef: master_def.to_string(),
                params: ParamMap::new(),
            },
        );
    }
    for (group, fx, def) in sides {
        script.add_group(
            *group,
            GroupConfig {
                name: format!("master{}/side{}", master.0, group.0),
                parent: Some(master),
                effects: fx.to_vec(),
                ..Default::default()
            },
        );
        for id in *fx {
            script.add_effect(
                *id,
                EffectConfig {
                    group: *group,
                    synthdef: def.to_string(),
                    params: ParamMap::new(),
                },
            );
        }
    }
    script
}

fn side(group: GroupId, fx: &'static [EffectId]) -> Side {
    Side { group, fx }
}

async fn apply(runtime: &mut Runtime<MockBackend>, script: ScriptState) {
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    runtime.tick().await;
}

/// Let the reload's deferred work land: removed effects and deleted groups
/// are faded and freed past a grace period, so the tree straight after an
/// `Apply` is mid-transition and not what the board ever plays. Every
/// assertion about audio-path order must be made on the SETTLED tree.
async fn settle(runtime: &mut Runtime<MockBackend>) {
    for _ in 0..4 {
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        runtime.tick().await;
    }
}

async fn boot_with_defs(
    script: ScriptState,
    defs: &[&str],
) -> (Runtime<MockBackend>, MockBackend) {
    let backend = MockBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    for def in defs {
        runtime
            .send(
                SynthDefMessage::Load {
                    name: (*def).to_string(),
                    data: Vec::new(),
                }
                .into(),
            )
            .await
            .unwrap();
    }
    runtime.tick().await;
    apply(&mut runtime, script).await;
    (runtime, backend)
}

async fn boot(script: ScriptState) -> (Runtime<MockBackend>, MockBackend) {
    boot_with_defs(script, &[DEF_FX]).await
}

fn report(breaks: &[AudioPathBreak]) -> String {
    breaks
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

// =========================================================================
// Tests
// =========================================================================

/// The factory bank, in miniature: masters that repeat, sides that move
/// between masters, and effects that migrate between a master chain and a
/// side chain.
fn bank() -> Vec<ScriptState> {
    vec![
        patch(
            MASTER_A,
            &[FX_1, FX_2],
            &[side(SIDE_A, &[FX_3]), side(SIDE_B, &[])],
        ),
        patch(MASTER_B, &[FX_2], &[side(SIDE_C, &[FX_1])]),
        patch(
            MASTER_A,
            &[FX_2, FX_1],
            &[side(SIDE_C, &[]), side(SIDE_A, &[FX_3])],
        ),
        patch(MASTER_A, &[FX_3], &[side(SIDE_B, &[FX_1, FX_2])]),
        patch(
            MASTER_B,
            &[FX_1, FX_3],
            &[side(SIDE_A, &[]), side(SIDE_B, &[FX_2])],
        ),
    ]
}

/// Baseline: a cold boot of the two-sides patch is connected.
#[tokio::test(flavor = "current_thread")]
async fn cold_boot_of_a_two_sided_patch_is_connected() {
    let (_runtime, backend) = boot(bank().remove(0)).await;
    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "cold boot must leave no node writing a bus nobody reads:\n{}",
        report(&breaks)
    );
}

/// The board defect: a recall that replaces the performance sides while the
/// master group survives. The new side groups must land before every node
/// that reads the master bus — its fx chain and its link synth.
#[tokio::test(flavor = "current_thread")]
async fn recall_that_replaces_the_sides_keeps_them_ahead_of_the_master_link() {
    let (mut runtime, backend) = boot(patch(
        MASTER_A,
        &[FX_1, FX_2],
        &[side(SIDE_A, &[FX_3]), side(SIDE_B, &[])],
    ))
    .await;
    assert!(
        backend.breaks().is_empty(),
        "precondition: cold boot is whole"
    );

    // The recall: same master, both sides replaced by new groups.
    apply(
        &mut runtime,
        patch(
            MASTER_A,
            &[FX_1, FX_2],
            &[side(SIDE_C, &[FX_3]), side(SIDE_A, &[])],
        ),
    )
    .await;

    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "a recall left a side writing a bus whose only reader already ran — \
         that side is SILENT with every synth alive:\n{}",
        report(&breaks)
    );
}

/// Many recalls in a row: the preset gate drives ~35 of them, and the board
/// only broke after a run of them.
#[tokio::test(flavor = "current_thread")]
async fn a_run_of_recalls_never_strands_a_side() {
    let bank = bank();
    let (mut runtime, backend) = boot(bank[0].clone()).await;

    for (round, script) in bank.iter().cycle().take(35).enumerate() {
        apply(&mut runtime, script.clone()).await;
        let breaks = backend.breaks();
        assert!(
            breaks.is_empty(),
            "recall #{round} stranded a side on a dead bus:\n{}",
            report(&breaks)
        );
    }
}

// =========================================================================
// The board shape: surviving master, changing child, recreated master fx.
// =========================================================================

/// `vibes-and-air` -> `horn-section`: master `sg_chain_n21` is declared by
/// BOTH patches, so its id is constant and it survives the reload. The child
/// is `sg_chain_n21/n17` in one and `sg_chain_n21/n18` in the other — ids
/// hash the full path, so the child is a NEW group. In the same reload the
/// surviving master's own effects change synthdef (`reverb_jpverb` ->
/// `hall_reverb`), so they are structurally recreated too.
const DEF_JPVERB: &str = "reverb_jpverb";
const DEF_HALL: &str = "hall_reverb";

/// Master fx in AUDIO-PATH order `[FX_3, FX_2, FX_1]` — the reverse of id
/// order, so the `min_by_key(id.raw())` fallback in
/// `first_effect_node_in_group` resolves to the chain TAIL (`FX_1`), not
/// the head. That is the anchor lead: a child group anchored `Before` the
/// tail fx lands after the fx that should have seen it.
const MASTER_CHAIN: [EffectId; 3] = [FX_3, FX_2, FX_1];

const CHILD_N17: GroupId = SIDE_A;
const CHILD_N18: GroupId = SIDE_B;

#[tokio::test(flavor = "current_thread")]
async fn board_shape_surviving_master_changing_child_recreated_fx() {
    let (mut runtime, backend) = boot_with_defs(
        patch_with_def(
            MASTER_A,
            &MASTER_CHAIN,
            DEF_JPVERB,
            &[(CHILD_N17, &[], DEF_JPVERB)],
        ),
        &[DEF_JPVERB, DEF_HALL],
    )
    .await;
    assert!(
        backend.breaks().is_empty(),
        "precondition: cold boot is whole:\n{}\n{}",
        backend.render(),
        report(&backend.breaks())
    );
    eprintln!("--- after cold boot (vibes-and-air) ---\n{}", backend.render());

    // The recall: same master id, NEW child id, master fx change synthdef.
    apply(
        &mut runtime,
        patch_with_def(
            MASTER_A,
            &MASTER_CHAIN,
            DEF_HALL,
            &[(CHILD_N18, &[], DEF_HALL)],
        ),
    )
    .await;
    eprintln!("--- after recall, unsettled ---\n{}", backend.render());
    settle(&mut runtime).await;
    eprintln!("--- after recall, settled (horn-section) ---\n{}", backend.render());

    let breaks = backend.breaks();
    assert!(
        breaks.is_empty(),
        "a recall left a child group writing a bus whose only reader already \
         ran — that child is SILENT with every synth alive:\n{}\n{}",
        backend.render(),
        report(&breaks)
    );
}

/// The factory bank in the real shape: masters that repeat under a constant
/// id, children whose ids change every recall, effects that MIGRATE between
/// a master chain and a child chain, and synthdefs that flip on the
/// surviving master so its chain is structurally recreated in place.
///
/// The migration is the part the old fixtures never had: once an effect id
/// listed in `State::group_effect_chain[master]` belongs to a CHILD instead,
/// `first_effect_node_in_group` finds nothing in the chain and falls through
/// to `min_by_key(id.raw())` over whatever effects are left on the master —
/// the smallest id hash, which is not the head of the audio path.
fn real_bank() -> Vec<ScriptState> {
    vec![
        // vibes-and-air: master chain [FX_3, FX_2, FX_1], child n17.
        patch_with_def(
            MASTER_A,
            &[FX_3, FX_2, FX_1],
            DEF_JPVERB,
            &[(CHILD_N17, &[], DEF_JPVERB)],
        ),
        // horn-section: same master id, NEW child id, defs flip, and FX_1
        // MIGRATES off the master onto the child.
        patch_with_def(
            MASTER_A,
            &[FX_3, FX_2],
            DEF_HALL,
            &[(CHILD_N18, &[FX_1], DEF_HALL)],
        ),
        // FX_1 comes back to the master, but at the chain TAIL, while FX_3
        // migrates out — so the min-id effect is no longer the chain head.
        patch_with_def(
            MASTER_A,
            &[FX_2, FX_1],
            DEF_JPVERB,
            &[(CHILD_N17, &[FX_3], DEF_JPVERB)],
        ),
        patch_with_def(
            MASTER_A,
            &[FX_1, FX_3],
            DEF_HALL,
            &[(CHILD_N18, &[FX_2], DEF_HALL)],
        ),
    ]
}

#[tokio::test(flavor = "current_thread")]
async fn a_run_of_real_shape_recalls_never_strands_a_child() {
    let bank = real_bank();
    let (mut runtime, backend) = boot_with_defs(bank[0].clone(), &[DEF_JPVERB, DEF_HALL]).await;
    settle(&mut runtime).await;

    for (round, script) in bank.iter().cycle().take(35).enumerate() {
        apply(&mut runtime, script.clone()).await;
        settle(&mut runtime).await;
        let breaks = backend.breaks();
        assert!(
            breaks.is_empty(),
            "recall #{round} stranded a child on a dead bus:\n{}\n{}",
            backend.render(),
            report(&breaks)
        );
    }
}
