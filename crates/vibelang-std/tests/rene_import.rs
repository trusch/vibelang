use std::fs;
use std::path::PathBuf;

use vibelang_dsp::{
    clear_synthdef_inputs_registry, clear_synthdef_outputs_registry, clear_synthdef_registry,
    get_synthdef_outputs, get_synthdef_param_defaults, set_deploy_callback, synthdef_exists,
    OutputPort, PortRate,
};
use vibelang_rhai::ScriptEngine;

#[test]
fn rene_import_registers_outputs_and_deepened_params() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_import_script("stdlib/instruments/eurorack/rene.vibe");

    let mut engine = ScriptEngine::new();
    engine.add_import_path(env!("CARGO_MANIFEST_DIR"));
    engine
        .execute_file(&script_path)
        .expect("Rene import should execute");
    fs::remove_file(&script_path).ok();

    assert!(synthdef_exists("rene"));
    assert_eq!(
        get_synthdef_outputs("rene"),
        Some(kr_ports(&["cv", "gate", "x_gate", "y_gate"]))
    );

    let params = get_synthdef_param_defaults("rene");
    assert_eq!(params.get("mode"), Some(&0.0));
    assert_eq!(params.get("quantize"), Some(&0.0));
    assert_eq!(params.get("scale_mask"), Some(&2741.0));
    assert_eq!(params.get("access_x"), Some(&15.0));
    assert_eq!(params.get("access_y"), Some(&15.0));
    assert_eq!(params.get("memory"), Some(&0.0));
    assert_eq!(params.get("random_seed"), Some(&0.0));
    assert_eq!(params.get("note00"), Some(&60.0));
    assert_eq!(params.get("note33"), Some(&72.0));
}

fn kr_ports(names: &[&str]) -> Vec<OutputPort> {
    names
        .iter()
        .map(|name| OutputPort {
            name: (*name).to_string(),
            channels: 1,
            rate: PortRate::Kr,
        })
        .collect()
}

fn write_import_script(import: &str) -> PathBuf {
    let path = temp_script_path();
    fs::write(&path, format!("import \"{}\";\n", import)).expect("write temp import script");
    path
}

fn temp_script_path() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vibelang-std-rene-import-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}
