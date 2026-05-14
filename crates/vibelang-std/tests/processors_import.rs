use std::fs;
use std::path::PathBuf;

use vibelang_dsp::{
    clear_synthdef_inputs_registry, clear_synthdef_outputs_registry, clear_synthdef_registry,
    get_synthdef_inputs, get_synthdef_outputs, set_deploy_callback, synthdef_exists, InputPort,
    OutputPort, PortRate,
};
use vibelang_rhai::ScriptEngine;

struct ProcessorCase {
    import_path: &'static str,
    synthdef: &'static str,
    inputs: &'static [(&'static str, u8)],
    out_channels: u8,
}

const CASES: &[ProcessorCase] = &[
    ProcessorCase {
        import_path: "stdlib/processors/utility/passthrough_mono.vibe",
        synthdef: "passthrough_mono",
        inputs: &[("in", 1)],
        out_channels: 1,
    },
    ProcessorCase {
        import_path: "stdlib/processors/utility/passthrough_stereo.vibe",
        synthdef: "passthrough_stereo",
        inputs: &[("in", 2)],
        out_channels: 2,
    },
    ProcessorCase {
        import_path: "stdlib/processors/filters/lowpass_mono.vibe",
        synthdef: "lowpass_mono",
        inputs: &[("in", 1)],
        out_channels: 1,
    },
    ProcessorCase {
        import_path: "stdlib/processors/filters/lowpass_stereo.vibe",
        synthdef: "lowpass_stereo",
        inputs: &[("in", 2)],
        out_channels: 2,
    },
    ProcessorCase {
        import_path: "stdlib/processors/modulation/ring_mod_mono.vibe",
        synthdef: "ring_mod_mono",
        inputs: &[("carrier", 1), ("modulator", 1)],
        out_channels: 1,
    },
    ProcessorCase {
        import_path: "stdlib/processors/modulation/ring_mod_stereo.vibe",
        synthdef: "ring_mod_stereo",
        inputs: &[("carrier", 2), ("modulator", 2)],
        out_channels: 2,
    },
    ProcessorCase {
        import_path: "stdlib/processors/mixers/crossfade_stereo.vibe",
        synthdef: "crossfade_stereo",
        inputs: &[("a", 2), ("b", 2)],
        out_channels: 2,
    },
    ProcessorCase {
        import_path: "stdlib/processors/mixers/mixer4_stereo.vibe",
        synthdef: "mixer4_stereo",
        inputs: &[("ch1", 2), ("ch2", 2), ("ch3", 2), ("ch4", 2)],
        out_channels: 2,
    },
];

#[test]
fn stdlib_processors_import_and_register_manifests() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_import_script(CASES);
    let mut engine = ScriptEngine::new();
    engine.add_import_path(env!("CARGO_MANIFEST_DIR"));
    engine
        .execute_file(&script_path)
        .expect("processor imports should execute");
    fs::remove_file(&script_path).ok();

    for case in CASES {
        assert!(
            synthdef_exists(case.synthdef),
            "{} should register a synthdef",
            case.synthdef
        );

        let expected_inputs = case
            .inputs
            .iter()
            .map(|(name, channels)| InputPort {
                name: (*name).to_string(),
                channels: *channels,
                rate: PortRate::Ar,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            get_synthdef_inputs(case.synthdef),
            Some(expected_inputs),
            "{} input manifest",
            case.synthdef
        );

        let outputs = get_synthdef_outputs(case.synthdef)
            .unwrap_or_else(|| panic!("{} should register output ports", case.synthdef));
        assert!(
            outputs.contains(&OutputPort {
                name: "out".to_string(),
                channels: case.out_channels,
                rate: PortRate::Ar,
            }),
            "{} should expose primary out:{} output, got {:?}",
            case.synthdef,
            case.out_channels,
            outputs
        );
    }
}

fn write_import_script(cases: &[ProcessorCase]) -> PathBuf {
    let mut script = String::new();
    for case in cases {
        script.push_str(&format!("import \"{}\";\n", case.import_path));
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
        "vibelang-std-processors-import-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}
