use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use async_trait::async_trait;
use vibelang_core::compat::Instant;
use vibelang_core::message::{ReloadMessage, SynthDefMessage};
use vibelang_core::{AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, Runtime};
use vibelang_rhai::ScriptEngine;

#[derive(Debug)]
struct MockError;

impl std::fmt::Display for MockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mock backend error")
    }
}

impl std::error::Error for MockError {}

#[derive(Debug, Default)]
struct RecordingBackend {
    creates: Mutex<Vec<String>>,
}

impl RecordingBackend {
    fn synth_creates(&self) -> Vec<String> {
        self.creates.lock().unwrap().clone()
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
        def: &str,
        _node: NodeId,
        _target: NodeId,
        _action: AddAction,
        _params: &ParamMap,
    ) -> Result<(), Self::Error> {
        self.creates.lock().unwrap().push(def.to_string());
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
        Ok(BufferInfo {
            frames: 0,
            channels: 1,
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

    fn current_time(&self) -> Instant {
        Instant::now()
    }
}

fn registry_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live under crates/vibelang-rhai")
        .to_path_buf()
}

fn temp_script_path() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vibelang-rhai-stdlib-synthdef-regression-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}

async fn execute_and_spawn(script: &str, synthdefs_to_load: &[&str]) -> Vec<String> {
    let root = project_root();
    let stdlib_root = root.join("crates/vibelang-std");
    let stdlib_dir = stdlib_root.join("stdlib");
    let script_path = temp_script_path();
    fs::write(&script_path, script).expect("write temp script");

    vibelang_dsp::set_deploy_callback(|_| Ok(()));

    let state = {
        let mut engine = ScriptEngine::new();
        engine.add_import_path(&root);
        engine.add_import_path(&stdlib_root);
        engine.add_import_path(&stdlib_dir);
        engine
            .execute_file(&script_path)
            .unwrap_or_else(|err| panic!("{} failed to execute: {err}", script_path.display()))
    };
    fs::remove_file(&script_path).ok();

    let mut runtime = Runtime::new(RecordingBackend::default());
    for name in synthdefs_to_load {
        runtime
            .send(
                SynthDefMessage::Load {
                    name: (*name).to_string(),
                    data: Vec::new(),
                }
                .into(),
            )
            .await
            .unwrap();
        runtime.tick().await;
    }

    runtime
        .send(ReloadMessage::Apply { state }.into())
        .await
        .unwrap();
    runtime.tick().await;
    runtime.backend().synth_creates()
}

async fn assert_snippet_spawns(script: &str, synthdef: &str) {
    let creates = execute_and_spawn(script, &[synthdef]).await;
    assert!(
        creates.iter().any(|name| name == synthdef),
        "expected {synthdef} node spawn, got {creates:?}",
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cv_seq_step_doc_usage_executes_and_spawns_node() {
    let _registry = registry_lock();

    assert_snippet_spawns(
        r#"
        import "stdlib/cv/seq/cv_seq_step.vibe" as cv_seq_step;

        let cv_seq_step = voice("doc_cv_seq_step")
            .synth("cv_seq_step")
            .set_param("clock_in", 1.0)
            .set_param("mode", 1.0)
            .set_param("length", 1.0)
            .set_param("note0", 60.0);
        cv_seq_step.output("out").to_main();
        cv_seq_step.run();
        "#,
        "cv_seq_step",
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn cv_euclidean_doc_usage_executes_and_spawns_node() {
    let _registry = registry_lock();

    assert_snippet_spawns(
        r#"
        import "stdlib/cv/triggers/cv_euclidean.vibe" as cv_euclidean;

        let cv_euclidean = voice("doc_cv_euclidean")
            .synth("cv_euclidean")
            .set_param("clock", 1.0)
            .set_param("hits", 0.5)
            .set_param("total", 0.5)
            .set_param("rotation", 0.5);
        cv_euclidean.output("out").to_main();
        cv_euclidean.run();
        "#,
        "cv_euclidean",
    )
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn vibrato_drift_doc_usage_executes_and_spawns_node() {
    let _registry = registry_lock();

    assert_snippet_spawns(
        r#"
        import "stdlib/effects/modulation/vibrato_drift.vibe" as vibrato_drift;

        let doc_vibrato_drift = fx("doc_vibrato_drift")
            .synth("vibrato_drift")
            .param("rate", 5.0)
            .param("depth", 0.002)
            .param("drift_amount", 0.5)
            .param("mix", 1.0)
            .apply();
        "#,
        "vibrato_drift",
    )
    .await;
}
