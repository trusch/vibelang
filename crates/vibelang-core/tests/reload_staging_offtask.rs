//! Reload staging: "the music never skips a beat during reload".
//!
//! Sample/SFZ buffer loads are the expensive part of a reload (file I/O
//! plus backend `/b_allocRead` round trips). They must run OFF the runtime
//! task — the same task drives pattern/melody scheduling at ~500Hz, so an
//! inline load audibly stalls playback. These tests pin three properties:
//!
//! 1. **Ticks keep running while a reload's samples load** — the runtime
//!    keeps processing unrelated messages while loads are gated, and the
//!    reload (group/entity creation) applies only after the loads
//!    complete, atomically on the runtime task.
//! 2. **Sample loads overlap** — the per-sample state lock is released
//!    across the backend await, so N loads take ~1 round trip, not N.
//!    (Previously a write lock held across the await serialized the
//!    surrounding `join_all`.)
//! 3. **`sync_and_wait` keeps its barrier meaning** — a sync sent after a
//!    reload must not resolve until that reload has actually applied,
//!    even though the reload now stages off-task (the CLI boots with
//!    `Apply` → `sync_and_wait` → transport start).
//!
//! A sample-free reload must still apply within a single tick (the fast
//! path every existing script edit takes).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use vibelang_core::reload::{GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, Message, NodeId, ParamMap, ReloadMessage,
    Runtime, SampleConfig, SampleId, SyncMessage, TransportMessage,
};

// =========================================================================
// Mock backend with a gate on load_buffer so tests control when "file
// loads" finish, plus concurrency accounting.
// =========================================================================

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock error")
    }
}

impl std::error::Error for MockError {}

#[derive(Default)]
struct BackendState {
    /// While false, load_buffer parks (simulating a slow disk/scsynth).
    release_loads: AtomicBool,
    loads_in_flight: AtomicU32,
    max_loads_in_flight: AtomicU32,
    loads_completed: AtomicU32,
}

#[derive(Clone, Default)]
struct GatedBackend {
    state: Arc<BackendState>,
}

impl GatedBackend {
    fn new() -> Self {
        Self::default()
    }

    fn release_loads(&self) {
        self.state.release_loads.store(true, Ordering::SeqCst);
    }

    fn loads_in_flight(&self) -> u32 {
        self.state.loads_in_flight.load(Ordering::SeqCst)
    }

    fn max_loads_in_flight(&self) -> u32 {
        self.state.max_loads_in_flight.load(Ordering::SeqCst)
    }

    fn loads_completed(&self) -> u32 {
        self.state.loads_completed.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Backend for GatedBackend {
    type Error = MockError;

    async fn load_synthdef(&self, _name: &str, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn create_synth(
        &self,
        _def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn create_group(
        &self,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn free_node(&self, _node: NodeId) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run_node(&self, _node: NodeId, _running: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn set_param(&self, _node: NodeId, _param: &str, _value: f32) -> Result<(), Self::Error> {
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
        let in_flight = self.state.loads_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.state
            .max_loads_in_flight
            .fetch_max(in_flight, Ordering::SeqCst);

        while !self.state.release_loads.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        self.state.loads_in_flight.fetch_sub(1, Ordering::SeqCst);
        self.state.loads_completed.fetch_add(1, Ordering::SeqCst);
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
    ) -> Result<BufferInfo, Self::Error> {
        Ok(BufferInfo {
            frames,
            channels,
            sample_rate: 44100.0,
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
// Fixtures
// =========================================================================

const GROUP: GroupId = GroupId(1);

fn script_with_samples(sample_ids: &[u32]) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            ..Default::default()
        },
    );
    for raw in sample_ids {
        script.add_sample(
            SampleId::new(*raw),
            SampleConfig::new(format!("/nonexistent/sample_{raw}.wav")),
        );
    }
    script
}

/// Tick the runtime until `predicate` holds, yielding to background tasks
/// between ticks. Panics after `max` iterations.
async fn tick_until<B, F, Fut>(runtime: &mut Runtime<B>, max: u32, what: &str, mut predicate: F)
where
    B: Backend,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..max {
        runtime.tick().await;
        if predicate().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("timed out waiting for: {what}");
}

// =========================================================================
// (1) + (2): staged loads leave the tick task free and run concurrently.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn reload_with_samples_keeps_ticking_and_loads_concurrently() {
    let backend = GatedBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let state = runtime.state().clone();

    runtime
        .send(
            ReloadMessage::Apply {
                state: script_with_samples(&[1, 2, 3]),
            }
            .into(),
        )
        .await
        .unwrap();

    // Let the runtime pick up the reload and spawn the staging task; the
    // gate is closed, so the "file loads" hang. All three loads must be
    // in flight AT THE SAME TIME: with the old lock-across-await bug the
    // loads serialized and in-flight never exceeded 1 (and, worse, they
    // ran inline on this very task, so this loop would deadlock).
    let mut reached_full_overlap = false;
    for _ in 0..500 {
        runtime.tick().await;
        if backend.loads_in_flight() == 3 {
            reached_full_overlap = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(
        reached_full_overlap,
        "all 3 sample loads must be in flight concurrently (max seen: {})",
        backend.max_loads_in_flight()
    );

    // While the loads hang, the runtime task must keep processing
    // unrelated messages — this is the "music never skips a beat" bit.
    runtime
        .send(Message::Transport(TransportMessage::SetTempo { bpm: 133.0 }))
        .await
        .unwrap();
    runtime.tick().await;
    assert_eq!(
        state.read().await.tempo,
        133.0,
        "tick task must keep handling messages while sample loads are staged"
    );

    // The reload itself must NOT have applied yet: buffers are confirmed
    // before any entity that references them is created.
    {
        let state = state.read().await;
        assert!(
            state.samples.is_empty(),
            "no sample may be published before its buffer load confirmed"
        );
        assert!(
            state.groups.is_empty(),
            "reload must apply atomically after staging, not piecemeal before"
        );
    }

    // Release the gate: the staged reload applies on a subsequent tick.
    backend.release_loads();
    tick_until(&mut runtime, 1000, "staged reload to apply", || {
        let state = state.clone();
        async move {
            let state = state.read().await;
            state.samples.len() == 3 && state.groups.contains_key(&GROUP)
        }
    })
    .await;

    assert_eq!(backend.loads_completed(), 3);

    // Distinct buffers per sample, allocated in sorted-sample-ID order
    // (deterministic cold boot).
    let state = state.read().await;
    let mut buffer_ids: Vec<u32> = state.samples.values().map(|s| s.buffer_id.0).collect();
    buffer_ids.sort_unstable();
    buffer_ids.dedup();
    assert_eq!(buffer_ids.len(), 3, "each sample gets its own buffer");
    let by_sample: Vec<u32> = {
        let mut ids: Vec<_> = state.samples.keys().copied().collect();
        ids.sort_by_key(|id| id.0);
        ids.iter().map(|id| state.samples[id].buffer_id.0).collect()
    };
    assert!(
        by_sample.windows(2).all(|w| w[0] < w[1]),
        "buffer IDs must be allocated in sorted sample-ID order: {by_sample:?}"
    );
}

// =========================================================================
// (3): sync_and_wait sent after a reload resolves only once it applied.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn sync_after_reload_defers_until_staged_reload_applies() {
    let backend = GatedBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let state = runtime.state().clone();

    runtime
        .send(
            ReloadMessage::Apply {
                state: script_with_samples(&[7]),
            }
            .into(),
        )
        .await
        .unwrap();

    let (notify_tx, mut notify_rx) = tokio::sync::oneshot::channel();
    runtime
        .send(Message::Sync(SyncMessage::SyncAndNotify { notify: notify_tx }))
        .await
        .unwrap();

    // With the load gated, the sync barrier must not resolve: the reload
    // that was sent before it has not applied yet.
    for _ in 0..50 {
        runtime.tick().await;
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    assert!(
        notify_rx.try_recv().is_err(),
        "sync sent after a reload must not resolve before the reload applied"
    );

    backend.release_loads();
    let mut notified = false;
    for _ in 0..1000 {
        runtime.tick().await;
        if notify_rx.try_recv().is_ok() {
            notified = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(notified, "sync barrier must resolve once the reload applied");

    // At notification time the reload state is fully visible.
    let state = state.read().await;
    assert_eq!(state.samples.len(), 1);
    assert!(state.groups.contains_key(&GROUP));
}

// =========================================================================
// Fast path: a sample-free reload still applies within a single tick.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn sample_free_reload_applies_within_one_tick() {
    let backend = GatedBackend::new();
    let mut runtime = Runtime::new(backend.clone());

    runtime
        .send(
            ReloadMessage::Apply {
                state: script_with_samples(&[]),
            }
            .into(),
        )
        .await
        .unwrap();
    runtime.tick().await;

    assert!(
        runtime.state().read().await.groups.contains_key(&GROUP),
        "a reload with nothing to stage must apply inline on the same tick"
    );
}
