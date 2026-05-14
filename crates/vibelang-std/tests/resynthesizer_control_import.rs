use std::fs;
use std::path::PathBuf;

use vibelang_dsp::{
    clear_synthdef_inputs_registry, clear_synthdef_outputs_registry, clear_synthdef_registry,
    get_synthdef_inputs, get_synthdef_outputs, get_synthdef_param_defaults, set_deploy_callback,
    synthdef_exists, InputPort, OutputPort, PortRate,
};
use vibelang_rhai::ScriptEngine;

#[test]
fn resynthesizer_control_modules_import_and_register_kr_outputs() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_import_script(&[
        "stdlib/instruments/eurorack/maths.vibe",
        "stdlib/instruments/eurorack/wogglebug.vibe",
        "stdlib/instruments/eurorack/tempi.vibe",
        "stdlib/instruments/eurorack/rene.vibe",
        "stdlib/instruments/eurorack/prss_pnt.vibe",
        "stdlib/instruments/eurorack/cv_bus.vibe",
    ]);

    let mut engine = ScriptEngine::new();
    engine.add_import_path(env!("CARGO_MANIFEST_DIR"));
    engine
        .execute_file(&script_path)
        .expect("ReSynthesizer control imports should execute");
    fs::remove_file(&script_path).ok();

    assert!(synthdef_exists("maths"));
    assert!(synthdef_exists("wogglebug"));
    assert!(synthdef_exists("tempi"));
    assert!(synthdef_exists("rene"));
    assert!(synthdef_exists("prss_pnt"));
    assert!(synthdef_exists("cv_bus"));

    assert_eq!(
        get_synthdef_outputs("maths"),
        Some(kr_ports(&[
            "ch1", "ch2", "ch3", "ch4", "sum", "or", "inv", "eor1", "eoc1", "eor4", "eoc4",
        ]))
    );
    assert_eq!(
        get_synthdef_outputs("wogglebug"),
        Some(kr_ports(&[
            "stepped",
            "smooth",
            "woggle",
            "clock",
            "woggle_clock",
        ]))
    );
    assert_eq!(
        get_synthdef_inputs("wogglebug"),
        Some(vec![InputPort::ar("influence", 1)])
    );
    assert_eq!(
        get_synthdef_outputs("tempi"),
        Some(kr_ports(&["ch1", "ch2", "ch3", "ch4", "ch5", "ch6"]))
    );
    assert_eq!(
        get_synthdef_outputs("rene"),
        Some(kr_ports(&["cv", "gate", "x_gate", "y_gate"]))
    );
    assert_eq!(
        get_synthdef_outputs("prss_pnt"),
        Some(kr_ports(&[
            "pressure1",
            "gate1",
            "pressure2",
            "gate2",
            "pressure3",
            "gate3",
            "pressure4",
            "gate4",
        ]))
    );
    assert_eq!(
        get_synthdef_outputs("cv_bus"),
        Some(kr_ports(&["bus1", "bus2", "bus3", "bus4", "sum"]))
    );

    let maths_params = get_synthdef_param_defaults("maths");
    assert_eq!(maths_params.get("cycle4"), Some(&0.0));
    assert_eq!(maths_params.get("ch2_scale"), Some(&0.5));
    assert_eq!(maths_params.get("ch2_offset"), Some(&0.0));
    assert_eq!(maths_params.get("ch3_offset"), Some(&0.0));
    assert_eq!(maths_params.get("rise1_cv"), Some(&0.0));
    assert_eq!(maths_params.get("rise1_cv_scale"), Some(&1.0));
    assert_eq!(maths_params.get("fall1_cv"), Some(&0.0));
    assert_eq!(maths_params.get("fall1_cv_scale"), Some(&1.0));
    assert_eq!(maths_params.get("rise4_cv"), Some(&0.0));
    assert_eq!(maths_params.get("rise4_cv_scale"), Some(&1.0));
    assert_eq!(maths_params.get("fall4_cv"), Some(&0.0));
    assert_eq!(maths_params.get("fall4_cv_scale"), Some(&1.0));

    let tempi_params = get_synthdef_param_defaults("tempi");
    assert_eq!(tempi_params.get("scene"), Some(&0.0));
    assert_eq!(tempi_params.get("mod"), Some(&1.0));
    assert_eq!(tempi_params.get("reset"), Some(&0.0));
    assert_eq!(tempi_params.get("reset_on_scene"), Some(&1.0));
    assert_eq!(tempi_params.get("variation"), Some(&0.0));
    assert_eq!(tempi_params.get("shift6"), Some(&0.0));
    assert_eq!(tempi_params.get("mute6"), Some(&0.0));
    assert_eq!(tempi_params.get("mult2"), Some(&2.0));

    let wogglebug_params = get_synthdef_param_defaults("wogglebug");
    assert_eq!(wogglebug_params.get("heart"), Some(&0.75));
    assert_eq!(wogglebug_params.get("vc_rate"), Some(&0.0));

    let rene_params = get_synthdef_param_defaults("rene");
    assert_eq!(rene_params.get("note00"), Some(&60.0));
    assert_eq!(rene_params.get("note33"), Some(&72.0));
}

#[test]
fn resynthesizer_wogglebug_import_resolves_from_installed_stdlib_parent() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_import_script(&["stdlib/instruments/eurorack/wogglebug.vibe"]);
    let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
    let stdlib_parent = stdlib_path
        .parent()
        .expect("stdlib install path should have parent")
        .to_path_buf();

    let mut engine = ScriptEngine::new();
    engine.add_import_path(stdlib_parent);
    engine
        .execute_file(&script_path)
        .expect("installed stdlib parent should resolve Wogglebug import");
    fs::remove_file(&script_path).ok();

    assert!(synthdef_exists("wogglebug"));
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
        "vibelang-std-resynth-control-import-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}
