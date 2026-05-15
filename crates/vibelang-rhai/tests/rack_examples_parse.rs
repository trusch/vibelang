use std::env;
use std::path::{Path, PathBuf};

use vibelang_rhai::ScriptEngine;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live under crates/vibelang-rhai")
        .to_path_buf()
}

fn rack_examples(root: &Path) -> Vec<PathBuf> {
    let mut examples = std::fs::read_dir(root.join("examples"))
        .expect("examples directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("main.vibe"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    examples.sort();
    examples
}

fn execute_example(path: &Path, root: &Path) {
    let stdlib_root = root.join("crates/vibelang-std");
    let stdlib_dir = stdlib_root.join("stdlib");

    vibelang_dsp::set_deploy_callback(|_| Ok(()));

    let mut engine = ScriptEngine::new();
    engine.add_import_path(root);
    engine.add_import_path(stdlib_root);
    engine.add_import_path(stdlib_dir);

    engine
        .execute_file(path)
        .unwrap_or_else(|err| panic!("{} failed to parse/execute: {err}", path.display()));
}

#[test]
#[ignore = "run through scripts/rack_smoke_audio_audit.py; rack examples are audited explicitly"]
fn rack_example_script_executes() {
    let root = project_root();

    if let Ok(example) = env::var("VIBE_RACK_EXAMPLE") {
        execute_example(Path::new(&example), &root);
        return;
    }

    let examples = rack_examples(&root);
    assert!(
        !examples.is_empty(),
        "expected at least one examples/*/main.vibe rack example"
    );

    for example in examples {
        execute_example(&example, &root);
    }
}
