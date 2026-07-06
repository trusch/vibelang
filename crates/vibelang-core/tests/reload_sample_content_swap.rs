//! In-place sample content reloads: fresh-ID load → swap → grace → free.
//!
//! Editing a sample's path (same sample name) or overwriting the file on
//! disk must actually reload the buffer on hot reload — reload must equal
//! cold boot. And the swap must be safe under playing notes: the new file
//! is loaded into a FRESH buffer ID, the `State::samples` mapping is
//! swapped so new note-ons resolve the new bufnum, and the OLD buffer is
//! `/b_free`d only after a grace period (matching
//! `BUFFER_FREE_GRACE_PERIOD_MS` in `handlers/samples.rs`) — never while
//! a live synth node may still be reading it, and its ID never returns to
//! the allocator pool before the free executes.
//!
//! Pinned properties:
//! 1. **Path change reloads** — same sample ID, new path: a new buffer ID
//!    is loaded, state points at it, the old buffer is freed only after
//!    the grace period.
//! 2. **Overwrite detection** — same path, newer mtime (captured into
//!    `SampleConfig::mtime` at script-eval time) diffs as updated and
//!    swaps buffers the same way.
//! 3. **No premature/spurious frees** — a buffer still referenced by the
//!    current state (unchanged sample) is never freed, and freed buffers
//!    never intersect the live sample mapping.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use vibelang_core::reload::{GroupConfig, ScriptState};
use vibelang_core::{
    AddAction, Backend, BufferId, BufferInfo, GroupId, NodeId, ParamMap, ReloadMessage, Runtime,
    SampleConfig, SampleId,
};

/// Must match `BUFFER_FREE_GRACE_PERIOD_MS` in
/// `crates/vibelang-core/src/handlers/samples.rs`.
const GRACE_MS: u64 = 500;

// =========================================================================
// Mock backend recording buffer loads and frees.
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
    loads: Mutex<Vec<(BufferId, String)>>,
    frees: Mutex<Vec<BufferId>>,
    loads_completed: AtomicU32,
    fail_loads: AtomicBool,
}

#[derive(Clone, Default)]
struct RecordingBackend {
    state: Arc<BackendState>,
}

impl RecordingBackend {
    fn new() -> Self {
        Self::default()
    }

    fn loads(&self) -> Vec<(BufferId, String)> {
        self.state.loads.lock().unwrap().clone()
    }

    fn freed(&self) -> Vec<BufferId> {
        self.state.frees.lock().unwrap().clone()
    }
}

#[async_trait]
impl Backend for RecordingBackend {
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

    async fn load_buffer(&self, id: BufferId, path: &Path) -> Result<BufferInfo, Self::Error> {
        if self.state.fail_loads.load(Ordering::SeqCst) {
            return Err(MockError);
        }
        self.state
            .loads
            .lock()
            .unwrap()
            .push((id, path.display().to_string()));
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

    async fn free_buffer(&self, id: BufferId) -> Result<(), Self::Error> {
        self.state.frees.lock().unwrap().push(id);
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
const SAMPLE: SampleId = SampleId(1);

fn mtime(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn script_with_sample(path: &str, file_mtime: Option<SystemTime>) -> ScriptState {
    let mut script = ScriptState::new();
    script.add_group(
        GROUP,
        GroupConfig {
            name: "g".to_string(),
            ..Default::default()
        },
    );
    let mut config = SampleConfig::new(path);
    config.mtime = file_mtime;
    script.add_sample(SAMPLE, config);
    script
}

/// Tick the runtime until `predicate` holds, yielding to background tasks
/// (the off-task staging load) between ticks. Panics after `max` rounds.
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

/// Sleep past the buffer grace period, then tick so the deferred frees drain.
async fn drain_grace_frees<B: Backend>(runtime: &mut Runtime<B>) {
    tokio::time::sleep(Duration::from_millis(GRACE_MS + 100)).await;
    runtime.tick().await;
}

async fn apply_and_wait_for_sample<F>(
    runtime: &mut Runtime<RecordingBackend>,
    script: ScriptState,
    what: &str,
    predicate: F,
) where
    F: Fn(&vibelang_core::SampleInfo) -> bool + Copy,
{
    let state = runtime.state().clone();
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    tick_until(runtime, 1000, what, || {
        let state = state.clone();
        async move {
            state
                .read()
                .await
                .samples
                .get(&SAMPLE)
                .is_some_and(predicate)
        }
    })
    .await;
}

// =========================================================================
// (1) In-place path change: fresh ID, swap, grace, free.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn sample_path_change_swaps_to_fresh_buffer_and_frees_old_after_grace() {
    let backend = RecordingBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let state = runtime.state().clone();

    apply_and_wait_for_sample(
        &mut runtime,
        script_with_sample("/samples/kick_v1.wav", Some(mtime(100))),
        "initial sample load",
        |info| info.path.to_string_lossy() == "/samples/kick_v1.wav",
    )
    .await;
    let old_buffer = state.read().await.samples[&SAMPLE].buffer_id;

    // Same sample ID, new path — must reload, not keep the old buffer.
    apply_and_wait_for_sample(
        &mut runtime,
        script_with_sample("/samples/kick_v2.wav", Some(mtime(100))),
        "path-change reload to swap the sample",
        |info| info.path.to_string_lossy() == "/samples/kick_v2.wav",
    )
    .await;

    let new_buffer = state.read().await.samples[&SAMPLE].buffer_id;
    assert_ne!(
        new_buffer, old_buffer,
        "updated sample must load into a FRESH buffer ID, not reuse the live one"
    );
    assert!(
        backend
            .loads()
            .iter()
            .any(|(id, path)| *id == new_buffer && path == "/samples/kick_v2.wav"),
        "new path must have been loaded into the new buffer; loads: {:?}",
        backend.loads()
    );

    // Immediately after the swap the old buffer must still be alive:
    // playing notes may read it.
    assert!(
        !backend.freed().contains(&old_buffer),
        "old buffer must NOT be freed before the grace period elapses"
    );

    drain_grace_frees(&mut runtime).await;
    assert!(
        backend.freed().contains(&old_buffer),
        "old buffer must be freed once the grace period elapsed; freed: {:?}",
        backend.freed()
    );
    assert!(
        !backend.freed().contains(&new_buffer),
        "the live buffer must never be freed"
    );
}

// =========================================================================
// (2) Same path, newer mtime (file overwritten on disk) is an update.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn sample_file_overwrite_same_path_newer_mtime_reloads_buffer() {
    let backend = RecordingBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let state = runtime.state().clone();

    apply_and_wait_for_sample(
        &mut runtime,
        script_with_sample("/samples/vocal.wav", Some(mtime(100))),
        "initial sample load",
        |info| info.source_mtime == Some(mtime(100)),
    )
    .await;
    let old_buffer = state.read().await.samples[&SAMPLE].buffer_id;
    let loads_before = backend.loads().len();

    // Same path, newer mtime — the file was re-recorded in place.
    apply_and_wait_for_sample(
        &mut runtime,
        script_with_sample("/samples/vocal.wav", Some(mtime(200))),
        "overwrite reload to swap the sample",
        |info| info.source_mtime == Some(mtime(200)),
    )
    .await;

    let new_buffer = state.read().await.samples[&SAMPLE].buffer_id;
    assert_ne!(
        new_buffer, old_buffer,
        "overwritten file (same path, newer mtime) must reload into a fresh buffer"
    );
    assert_eq!(
        backend.loads().len(),
        loads_before + 1,
        "exactly one new load for the overwritten file; loads: {:?}",
        backend.loads()
    );

    assert!(
        !backend.freed().contains(&old_buffer),
        "old buffer must NOT be freed before the grace period elapses"
    );
    drain_grace_frees(&mut runtime).await;
    assert!(
        backend.freed().contains(&old_buffer),
        "old buffer must be freed after the grace period; freed: {:?}",
        backend.freed()
    );
}

// =========================================================================
// (3) Unchanged samples are neither reloaded nor freed; freed buffers
//     never intersect the live mapping.
// =========================================================================

#[tokio::test(flavor = "current_thread")]
async fn unchanged_sample_buffer_is_never_reloaded_or_freed() {
    let backend = RecordingBackend::new();
    let mut runtime = Runtime::new(backend.clone());
    let state = runtime.state().clone();

    apply_and_wait_for_sample(
        &mut runtime,
        script_with_sample("/samples/snare.wav", Some(mtime(100))),
        "initial sample load",
        |_| true,
    )
    .await;
    let buffer = state.read().await.samples[&SAMPLE].buffer_id;
    let loads_before = backend.loads().len();

    // Unrelated reload: same sample (same path, same mtime), extra group.
    let mut script = script_with_sample("/samples/snare.wav", Some(mtime(100)));
    script.add_group(
        GroupId(2),
        GroupConfig {
            name: "g2".to_string(),
            ..Default::default()
        },
    );
    runtime
        .send(ReloadMessage::Apply { state: script }.into())
        .await
        .unwrap();
    tick_until(&mut runtime, 1000, "unrelated reload to apply", || {
        let state = state.clone();
        async move { state.read().await.groups.contains_key(&GroupId(2)) }
    })
    .await;

    assert_eq!(
        backend.loads().len(),
        loads_before,
        "an unchanged sample must not be reloaded by an unrelated reload"
    );
    assert_eq!(
        state.read().await.samples[&SAMPLE].buffer_id,
        buffer,
        "an unchanged sample must keep its buffer across an unrelated reload"
    );

    drain_grace_frees(&mut runtime).await;
    assert!(
        !backend.freed().contains(&buffer),
        "a buffer the current state still references must never be freed; freed: {:?}",
        backend.freed()
    );

    // Invariant across the whole test: nothing freed is still live.
    let live: Vec<BufferId> = state
        .read()
        .await
        .samples
        .values()
        .map(|s| s.buffer_id)
        .collect();
    assert!(
        backend.freed().iter().all(|freed| !live.contains(freed)),
        "freed buffers must never intersect the live sample mapping; freed: {:?}, live: {:?}",
        backend.freed(),
        live
    );
}
