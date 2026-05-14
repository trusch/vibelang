use std::fs;
use std::path::PathBuf;

use vibelang_dsp::{
    clear_synthdef_inputs_registry, clear_synthdef_outputs_registry, clear_synthdef_registry,
    set_deploy_callback,
};
use vibelang_rhai::ScriptEngine;

#[test]
fn resynthesizer_patch_surface_wires_named_ar_inputs_and_kr_params() {
    clear_synthdef_registry();
    clear_synthdef_inputs_registry();
    clear_synthdef_outputs_registry();
    set_deploy_callback(|_| Ok(()));

    let script_path = write_patch_surface_script();
    let mut engine = ScriptEngine::new();
    engine.add_import_path(env!("CARGO_MANIFEST_DIR"));
    let state = engine
        .execute_file(&script_path)
        .expect("ReSynthesizer patch surface should execute");
    fs::remove_file(&script_path).ok();

    let spectraphon = *state
        .voices
        .iter()
        .find(|(_, voice)| voice.name == "resynth_surface_spectraphon")
        .expect("spectraphon voice should be present")
        .0;
    let morphagene = *state
        .voices
        .iter()
        .find(|(_, voice)| voice.name == "resynth_surface_morphagene")
        .expect("morphagene voice should be present")
        .0;
    let maths = *state
        .voices
        .iter()
        .find(|(_, voice)| voice.name == "resynth_surface_maths")
        .expect("maths voice should be present")
        .0;
    let x_pan = *state
        .voices
        .iter()
        .find(|(_, voice)| voice.name == "resynth_surface_x_pan")
        .expect("x-pan voice should be present")
        .0;
    let qpas = *state
        .voices
        .iter()
        .find(|(_, voice)| voice.name == "resynth_surface_qpas")
        .expect("qpas voice should be present")
        .0;

    assert_eq!(state.voices[&spectraphon].synthdef, "spectraphon_side");
    assert_eq!(state.voices[&morphagene].synthdef, "morphagene");
    assert_eq!(state.voices[&maths].synthdef, "maths");
    assert_eq!(state.voices[&x_pan].synthdef, "x_pan");
    assert_eq!(state.voices[&qpas].synthdef, "qpas");

    assert_route_to_group(
        state.routes.get(&(spectraphon, "odd".to_string())),
        "spectraphon.odd",
    );
    assert_route_to_group(
        state.routes.get(&(spectraphon, "even".to_string())),
        "spectraphon.even",
    );
    assert_route_to_group(
        state.routes.get(&(morphagene, "left".to_string())),
        "morphagene.left",
    );
    assert_route_to_group(
        state.routes.get(&(morphagene, "right".to_string())),
        "morphagene.right",
    );
    assert_route_to_group(state.routes.get(&(x_pan, "out".to_string())), "x_pan.out");
    assert_route_to_main(state.routes.get(&(qpas, "out".to_string())), "qpas.out");

    assert_input_from_group(
        state.input_routes.get(&(x_pan, "ch1_a".to_string())),
        "x_pan.ch1_a",
    );
    assert_input_from_group(
        state.input_routes.get(&(x_pan, "ch1_b".to_string())),
        "x_pan.ch1_b",
    );
    assert_input_from_group(
        state.input_routes.get(&(x_pan, "aux".to_string())),
        "x_pan.aux",
    );
    assert_input_from_voice_out(
        state.input_routes.get(&(qpas, "in".to_string())),
        "qpas.in",
        x_pan,
    );

    assert_param_route(
        state.param_routes_set.get(&(maths, "sum".to_string())),
        "maths.sum",
        spectraphon,
        "focus",
    );
    assert_param_route(
        state.param_routes_set.get(&(maths, "inv".to_string())),
        "maths.inv",
        x_pan,
        "ch1_pan",
    );
    assert_param_route(
        state
            .param_routes_set
            .get(&(morphagene, "eosg".to_string())),
        "morphagene.eosg",
        maths,
        "trig1",
    );
}

fn assert_route_to_group(route: Option<&impl std::fmt::Debug>, label: &str) {
    let debug = format!("{route:?}");
    assert!(
        debug.contains("Group("),
        "{label} should route to a group, route={debug}"
    );
}

fn assert_route_to_main(route: Option<&impl std::fmt::Debug>, label: &str) {
    let debug = format!("{route:?}");
    assert!(
        debug.contains("Main"),
        "{label} should route to main, route={debug}"
    );
}

fn assert_input_from_group(input_route: Option<&impl std::fmt::Debug>, label: &str) {
    let debug = format!("{input_route:?}");
    assert!(
        debug.contains("Group("),
        "{label} should read from a group, input_route={debug}"
    );
}

fn assert_input_from_voice_out(
    input_route: Option<&impl std::fmt::Debug>,
    label: &str,
    source: impl std::fmt::Debug,
) {
    let debug = format!("{input_route:?}");
    let source_debug = format!("{source:?}");
    assert!(
        debug.contains(&format!("Voice({source_debug}, \"out\")")),
        "{label} should read from {source_debug}.out, input_route={debug}"
    );
}

fn assert_param_route(
    param_route: Option<&impl std::fmt::Debug>,
    label: &str,
    target: impl std::fmt::Debug,
    target_param: &str,
) {
    let debug = format!("{param_route:?}");
    let target_debug = format!("{target:?}");
    assert!(
        debug.contains(&format!("Voice({target_debug})"))
            && debug.contains(&format!("\"{target_param}\"")),
        "{label} should route to {target_debug}.{target_param}, param_route={debug}"
    );
}

fn write_patch_surface_script() -> PathBuf {
    let script = r#"
import "stdlib/instruments/spectral/spectraphon_side.vibe";
import "stdlib/instruments/spectral/spectraphon_dual.vibe";
import "stdlib/instruments/sampler/morphagene.vibe";
import "stdlib/instruments/eurorack/maths.vibe";
import "stdlib/instruments/eurorack/wogglebug.vibe";
import "stdlib/instruments/eurorack/tempi.vibe";
import "stdlib/instruments/eurorack/rene.vibe";
import "stdlib/instruments/eurorack/prss_pnt.vibe";
import "stdlib/instruments/eurorack/cv_bus.vibe";
import "stdlib/processors/mixers/x_pan.vibe";
import "stdlib/processors/filters/qpas.vibe";
import "stdlib/processors/dynamics/dxg.vibe";
import "stdlib/processors/delays/mimeophon.vibe";

define_group("resynth_surface_sources", || { });
define_group("resynth_surface_spectral", || { });
define_group("resynth_surface_morph", || { });
define_group("resynth_surface_rack", || { });

let maths = voice("resynth_surface_maths")
    .synth("maths")
    .group("resynth_surface_sources")
    .modulator_only()
    .set_param("cycle1", 1.0)
    .set_param("cycle4", 1.0);

let spectraphon = voice("resynth_surface_spectraphon")
    .synth("spectraphon_side")
    .group("resynth_surface_sources")
    .set_param("freq", 146.8)
    .set_param("partials", 0.62)
    .set_param("focus", 0.48);

let morphagene = voice("resynth_surface_morphagene")
    .synth("morphagene")
    .group("resynth_surface_sources")
    .set_param("organize", 0.25)
    .set_param("gene_size", 0.32)
    .set_param("morph", 0.58);

spectraphon.output("odd").to(group("resynth_surface_spectral"));
spectraphon.output("even").to(group("resynth_surface_spectral"));
morphagene.output("left").to(group("resynth_surface_morph"));
morphagene.output("right").to(group("resynth_surface_morph"));

maths.output("sum").to_param(spectraphon, "focus").scale(0.4).offset(0.35);
morphagene.output("eosg").to_param(maths, "trig1");

let x_pan = voice("resynth_surface_x_pan")
    .synth("x_pan")
    .group("resynth_surface_rack")
    .set_param("ch1_gain", 0.75)
    .set_param("aux_gain", 0.4);

x_pan.input("ch1_a").from(group("resynth_surface_spectral"));
x_pan.input("ch1_b").from(group("resynth_surface_morph"));
x_pan.input("aux").from(group("resynth_surface_morph"));
x_pan.output("out").to(group("resynth_surface_rack"));
maths.output("inv").to_param(x_pan, "ch1_pan").scale(0.65).offset(-0.1);

let qpas = voice("resynth_surface_qpas")
    .synth("qpas")
    .group("resynth_surface_rack")
    .set_param("cutoff", 1600.0)
    .set_param("q", 0.35)
    .set_param("radiate", 0.25);

qpas.input("in").from(x_pan);
qpas.output("out").to_main();

voice("resynth_surface_spectraphon_dual").synth("spectraphon_dual").group("resynth_surface_sources");
voice("resynth_surface_wogglebug").synth("wogglebug").group("resynth_surface_sources").modulator_only();
voice("resynth_surface_tempi").synth("tempi").group("resynth_surface_sources").modulator_only();
voice("resynth_surface_rene").synth("rene").group("resynth_surface_sources").modulator_only();
voice("resynth_surface_prss_pnt").synth("prss_pnt").group("resynth_surface_sources").modulator_only();
voice("resynth_surface_cv_bus").synth("cv_bus").group("resynth_surface_sources").modulator_only();
voice("resynth_surface_dxg").synth("dxg").group("resynth_surface_rack");
voice("resynth_surface_mimeophon").synth("mimeophon").group("resynth_surface_rack");
"#;

    let path = temp_script_path();
    fs::write(&path, script).expect("write temp ReSynthesizer patch surface script");
    path
}

fn temp_script_path() -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "vibelang-std-resynth-patch-surface-{}-{}.vibe",
        std::process::id(),
        nonce
    ))
}
