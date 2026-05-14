use std::fs;
use std::path::PathBuf;

use vibelang_dsp::{
    clear_synthdef_inputs_registry, clear_synthdef_outputs_registry, clear_synthdef_registry,
    get_synthdef_outputs, get_synthdef_param_defaults, set_deploy_callback, synthdef_exists,
    OutputPort, PortRate,
};
use vibelang_rhai::ScriptEngine;

#[test]
fn resynthesizer_core_modules_import_and_register_outputs() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_import_script(&[
        "stdlib/instruments/spectral/spectraphon_side.vibe",
        "stdlib/instruments/spectral/spectraphon_dual.vibe",
        "stdlib/instruments/sampler/morphagene.vibe",
    ]);

    let mut engine = ScriptEngine::new();
    engine.add_import_path(env!("CARGO_MANIFEST_DIR"));
    engine
        .execute_file(&script_path)
        .expect("ReSynthesizer core imports should execute");
    fs::remove_file(&script_path).ok();

    assert!(synthdef_exists("spectraphon_side"));
    assert!(synthdef_exists("spectraphon_dual"));
    assert!(synthdef_exists("morphagene"));

    assert_eq!(
        get_synthdef_outputs("spectraphon_side"),
        Some(ar_ports(&["sine", "sub", "odd", "even"]))
    );
    assert_eq!(
        get_synthdef_outputs("spectraphon_dual"),
        Some(ar_ports(&["odd_a", "even_a", "odd_b", "even_b"]))
    );
    let dual_params = get_synthdef_param_defaults("spectraphon_dual");
    assert_eq!(dual_params.get("bufnum"), Some(&0.0));
    assert_eq!(dual_params.get("sync"), Some(&0.0));
    assert_eq!(dual_params.get("sam_capture_a"), Some(&0.0));
    assert_eq!(dual_params.get("sam_capture_b"), Some(&0.0));
    assert_eq!(dual_params.get("array_idx_a"), Some(&0.0));
    assert_eq!(dual_params.get("array_idx_b"), Some(&0.0));
    assert_eq!(
        get_synthdef_outputs("morphagene"),
        Some(vec![
            OutputPort {
                name: "left".to_string(),
                channels: 1,
                rate: PortRate::Ar,
            },
            OutputPort {
                name: "right".to_string(),
                channels: 1,
                rate: PortRate::Ar,
            },
            OutputPort {
                name: "eosg".to_string(),
                channels: 1,
                rate: PortRate::Kr,
            },
        ])
    );

    let morph_params = get_synthdef_param_defaults("morphagene");
    assert_eq!(morph_params.get("amp"), Some(&0.5));
    assert_eq!(morph_params.get("auto_level"), Some(&0.0));
    assert_eq!(morph_params.get("play_gate"), Some(&1.0));
    assert_eq!(morph_params.get("splices_buf"), Some(&0.0));
}

fn ar_ports(names: &[&str]) -> Vec<OutputPort> {
    names
        .iter()
        .map(|name| OutputPort {
            name: (*name).to_string(),
            channels: 1,
            rate: PortRate::Ar,
        })
        .collect()
}

fn write_import_script(imports: &[&str]) -> PathBuf {
    let mut script = String::new();
    for import in imports {
        script.push_str(&format!("import \"{}\";\n", import));
    }

    let path = temp_script_path();
    fs::write(&path, script).expect("write temp import script");
    path
}

fn temp_script_path() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vibelang-std-resynth-core-import-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}
