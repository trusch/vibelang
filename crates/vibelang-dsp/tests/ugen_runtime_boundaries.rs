#[allow(dead_code)]
#[path = "../build_support.rs"]
mod build_support;

use build_support::{
    has_array_overload, positional_arity_max, runtime_rate_rust, to_snake_case, ugen_numeric_range,
    UGenManifest,
};
use rhai::{Dynamic, Engine};
use serde::Deserialize;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};
use vibelang_dsp::{
    clear_active_builder, register_dsp_api, register_dsp_api_v2, set_active_builder,
    GraphBuilderInner, Input, Rate,
};

const MANIFESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/ugen_manifests");
const EFFECTIVE_METADATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../api/effective-metadata-v1.json"
);
const EXPECTED_FUNCTIONS: usize = 1_174;
const EXPECTED_OVERLOADS: usize = 5_962;
const EXPECTED_FALLBACK_EXPOSURES: usize = 4_788;
const EXPECTED_POTENTIAL_PANIC_EXPOSURES: usize = 5_962;
const COMPAT_DIAGNOSTIC: &str = "diagnostic.compat.dsp_ugen_recovery";

fn load_manifests() -> Vec<UGenManifest> {
    let mut paths: Vec<PathBuf> = fs::read_dir(Path::new(MANIFESTS_DIR))
        .expect("read UGen manifests")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect();
    paths.sort();

    paths
        .into_iter()
        .flat_map(|path| {
            let body = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            serde_json::from_str::<Vec<UGenManifest>>(&body)
                .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
        })
        .collect()
}

fn concrete_functions(manifests: &[UGenManifest]) -> impl Iterator<Item = (String, &UGenManifest)> {
    manifests.iter().flat_map(|manifest| {
        manifest
            .rates
            .iter()
            .filter(|rate| runtime_rate_rust(rate).is_some())
            .map(move |rate| {
                (
                    format!("{}_{}", to_snake_case(&manifest.name), rate),
                    manifest,
                )
            })
    })
}

fn positional_call(function: &str, arity: usize) -> String {
    format!(
        "{function}({})",
        std::iter::repeat_n("1.0", arity)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn array_call(function: &str, arity: usize) -> String {
    format!(
        "{function}([{}])",
        std::iter::repeat_n("1.0", arity)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[test]
fn strict_v2_exercises_every_audited_ugen_overload_without_unwinding() {
    let manifests = load_manifests();
    let mut engine = Engine::new();
    register_dsp_api_v2(&mut engine);

    let mut function_count = 0;
    let mut overload_count = 0;
    let mut fallback_count = 0;
    let mut potential_panic_exposure_count = 0;
    let mut failures = Vec::new();

    for (function, manifest) in concrete_functions(&manifests) {
        function_count += 1;
        let input_count = manifest.inputs.len();
        for arity in 0..=positional_arity_max(input_count) {
            let script = positional_call(&function, arity);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                engine.eval_expression::<Dynamic>(&script)
            }));
            overload_count += 1;
            potential_panic_exposure_count += 1;
            match outcome {
                Err(_) => failures.push(format!("{script}: unwound")),
                Ok(Ok(_)) => failures.push(format!("{script}: unexpectedly succeeded")),
                Ok(Err(error)) if arity < input_count => {
                    fallback_count += 1;
                    if !error.to_string().contains("dsp.ugen.argument.omitted") {
                        failures.push(format!("{script}: {error}"));
                    }
                }
                Ok(Err(_)) => {}
            }
        }

        if has_array_overload(input_count) {
            let script = array_call(&function, input_count);
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                engine.eval_expression::<Dynamic>(&script)
            }));
            overload_count += 1;
            potential_panic_exposure_count += 1;
            match outcome {
                Err(_) => failures.push(format!("{script}: unwound")),
                Ok(Ok(_)) => failures.push(format!("{script}: unexpectedly succeeded")),
                Ok(Err(_)) => {}
            }
        }
    }

    assert_eq!(function_count, EXPECTED_FUNCTIONS);
    assert_eq!(overload_count, EXPECTED_OVERLOADS);
    assert_eq!(fallback_count, EXPECTED_FALLBACK_EXPOSURES);
    assert_eq!(
        potential_panic_exposure_count,
        EXPECTED_POTENTIAL_PANIC_EXPOSURES
    );
    assert!(
        failures.is_empty(),
        "{} strict generated adapter failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn strict_error(script: &str, expected_id: &str) {
    let mut engine = Engine::new();
    register_dsp_api_v2(&mut engine);
    engine
        .register_fn("nan_value", || f64::NAN)
        .register_fn("infinity", || f64::INFINITY)
        .register_fn("negative_infinity", || f64::NEG_INFINITY)
        .register_fn("too_large", || f32::MAX as f64 * 2.0)
        .register_fn("underflow", || f64::from_bits(1));

    set_active_builder(GraphBuilderInner::new());
    let outcome = catch_unwind(AssertUnwindSafe(|| engine.eval::<Dynamic>(script)));
    let builder = clear_active_builder().expect("active builder vanished");
    let result = outcome.unwrap_or_else(|_| panic!("{script} unwound"));
    let error = result.unwrap_err().to_string();

    assert!(
        error.contains(expected_id),
        "{script}: expected {expected_id}, got {error}"
    );
    assert!(
        builder.nodes.is_empty() && builder.constants.is_empty(),
        "{script}: strict rejection reached graph dispatch"
    );
}

#[test]
fn strict_v2_rejects_numeric_invalid_values_before_graph_dispatch() {
    for value in ["nan_value()", "infinity()", "negative_infinity()"] {
        strict_error(
            &format!("sin_osc_ar({value}, 0.0)"),
            "dsp.ugen.numeric.non_finite",
        );
    }
    strict_error("dc_ar(nan_value())", "dsp.ugen.numeric.non_finite");
    for value in ["too_large()", "underflow()"] {
        strict_error(
            &format!("sin_osc_ar({value}, 0.0)"),
            "dsp.ugen.numeric.out_of_f32_range",
        );
    }
    strict_error("mfcc_kr(1.0, 3.5)", "dsp.ugen.numeric.not_integral");
    strict_error("sin_osc_ar(0.0, 0.0)", "dsp.ugen.numeric.below_range");
    strict_error("out_ar(-1.0, 1.0)", "dsp.ugen.numeric.below_range");
    strict_error("in_ar(-1.0, 1.0)", "dsp.ugen.numeric.below_range");
    strict_error("in_ar(16777217.0, 1.0)", "dsp.helper.in_ar.bus");
    strict_error(
        "sound_in_channel(16777216.5)",
        "dsp.helper.sound_in.channel",
    );
    strict_error("sound_in_channel(16777217)", "dsp.helper.sound_in.channel");
    strict_error(
        "in_ar(0.0, 32768.0)",
        "Expected channel count integer in 1..=32767",
    );
    strict_error(
        "play_buf_ar(32768.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0)",
        "Expected channel count integer in 1..=32767",
    );
}

struct DiagnosticLogger {
    messages: Mutex<Vec<String>>,
}

impl log::Log for DiagnosticLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "vibelang::compat"
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.messages
                .lock()
                .unwrap()
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

static LOGGER: DiagnosticLogger = DiagnosticLogger {
    messages: Mutex::new(Vec::new()),
};
static LOGGER_INIT: Once = Once::new();

fn reset_diagnostics() {
    LOGGER_INIT.call_once(|| {
        log::set_logger(&LOGGER).expect("install compatibility diagnostic logger");
        log::set_max_level(log::LevelFilter::Warn);
    });
    LOGGER.messages.lock().unwrap().clear();
}

#[test]
fn v1_recovers_legacy_values_with_exactly_one_stable_diagnostic() {
    reset_diagnostics();
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);
    engine.register_fn("nan_value", || f64::NAN);

    set_active_builder(GraphBuilderInner::new());
    let _ = engine
        .eval::<Dynamic>("mfcc_kr(nan_value())")
        .expect("v1 compatibility call should retain its effective values");
    let builder = clear_active_builder().expect("active builder vanished");

    let node = builder
        .nodes
        .iter()
        .find(|node| node.name == "MFCC")
        .expect("MFCC node missing");
    assert!(matches!(node.inputs.first(), Some(Input::Constant(value)) if value.is_nan()));
    assert!(matches!(node.inputs.get(1), Some(Input::Constant(13.0))));

    let messages = LOGGER.messages.lock().unwrap();
    assert_eq!(messages.len(), 1, "diagnostics: {messages:?}");
    assert_eq!(
        messages[0],
        "diagnostic.compat.dsp_ugen_recovery profile=compat.vibelang.v1 function=mfcc_kr arguments=chain,numcoeff reasons=dsp.ugen.numeric.non_finite,dsp.ugen.argument.omitted_manifest_default recovery=legacy_effective_values replacement=supply_explicit_finite_in_range_arguments"
    );
    drop(messages);

    for (function, rate) in [("dc_ar", Rate::Audio), ("dc_kr", Rate::Control)] {
        reset_diagnostics();
        set_active_builder(GraphBuilderInner::new());
        let _ = engine
            .eval::<Dynamic>(&format!("{function}(nan_value())"))
            .expect("v1 typed DC compatibility call should retain its effective value");
        let builder = clear_active_builder().expect("active builder vanished");

        let node = builder
            .nodes
            .iter()
            .find(|node| node.name == "DC")
            .expect("DC node missing");
        assert_eq!(node.rate, rate);
        assert!(matches!(node.inputs.first(), Some(Input::Constant(value)) if value.is_nan()));

        let messages = LOGGER.messages.lock().unwrap();
        assert_eq!(messages.len(), 1, "diagnostics: {messages:?}");
        assert_eq!(
            messages[0],
            format!(
                "diagnostic.compat.dsp_ugen_recovery profile=compat.vibelang.v1 function={function} arguments=in reasons=dsp.ugen.numeric.non_finite recovery=legacy_effective_values replacement=supply_explicit_finite_in_range_arguments"
            )
        );
    }
}

#[derive(Deserialize)]
struct EffectiveMetadata {
    ugen_input_quantities: Vec<QuantityOccurrence>,
}

#[derive(Deserialize)]
struct QuantityOccurrence {
    classification: QuantityClassification,
}

#[derive(Deserialize)]
struct QuantityClassification {
    range_id: Option<String>,
    provenance: Option<Vec<String>>,
}

#[test]
fn generated_profiles_preserve_v1_high_arity_metadata_shape() {
    let source = include_str!(concat!(env!("OUT_DIR"), "/generated.rs"));
    let compatibility_marker = concat!(
        "helpers::UgenAdapterProfile::V1Compatibility => {\n",
        "            engine.register_fn(\n",
        "                \"mi_elements_ar\",\n",
        "                move |"
    );
    let strict_marker = concat!(
        "helpers::UgenAdapterProfile::V2Strict => {\n",
        "            engine.register_fn(\n",
        "                \"mi_elements_ar\",\n",
        "                move |"
    );
    let compatibility_params = source
        .split_once(compatibility_marker)
        .expect("v1 mi_elements_ar high-arity registration")
        .1
        .split_once("| {")
        .expect("v1 mi_elements_ar closure")
        .0;
    let strict_params = source
        .split_once(strict_marker)
        .expect("v2 mi_elements_ar high-arity registration")
        .1
        .split_once("| {")
        .expect("v2 mi_elements_ar closure")
        .0;

    assert_eq!(compatibility_params.matches(": Dynamic").count(), 17);
    assert!(!compatibility_params.contains(": f64"));
    assert_eq!(strict_params.matches(": Dynamic").count(), 16);
    assert_eq!(strict_params.matches(": f64").count(), 1);
}

#[test]
fn generated_numeric_ranges_match_the_accepted_effective_metadata() {
    let manifests = load_manifests();
    let metadata: EffectiveMetadata = serde_json::from_str(
        &fs::read_to_string(EFFECTIVE_METADATA).expect("read accepted effective metadata"),
    )
    .expect("parse accepted effective metadata");

    let mut checked = 0;
    let mut failures = Vec::new();
    for (function, manifest) in concrete_functions(&manifests) {
        for input in &manifest.inputs {
            let provenance = format!("dsp_ugen:{function}.{} input_type={}", input.name, input.ty);
            let accepted = metadata
                .ugen_input_quantities
                .iter()
                .find(|occurrence| {
                    occurrence
                        .classification
                        .provenance
                        .as_deref()
                        .is_some_and(|items| items.iter().any(|item| item == &provenance))
                })
                .and_then(|occurrence| occurrence.classification.range_id.as_deref());
            let generated = ugen_numeric_range(&function, input).map(|range| match range {
                build_support::UGenNumericRange::FiniteUnbounded => "range.finite.unbounded",
                build_support::UGenNumericRange::FiniteNonnegative => "range.finite.nonnegative",
                build_support::UGenNumericRange::FinitePositive => "range.finite.positive",
                build_support::UGenNumericRange::IntegerUnbounded => "range.integer.unbounded",
                build_support::UGenNumericRange::IntegerNonnegative => "range.integer.nonnegative",
            });
            checked += 1;
            if accepted != generated {
                failures.push(format!(
                    "{function}.{}: accepted={accepted:?} generated={generated:?}",
                    input.name
                ));
            }
        }
    }

    assert!(checked > EXPECTED_FUNCTIONS);
    assert!(
        failures.is_empty(),
        "{} generated range classification drift(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(COMPAT_DIAGNOSTIC, "diagnostic.compat.dsp_ugen_recovery");
}
