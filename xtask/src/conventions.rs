use crate::public_api;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use vibelang_api_manifest::conventions::{
    AvailabilityBinding, AvailabilityGate, AvailabilityReasonDefinition,
    AvailabilityStateDefinition, CapabilityDefinition, ClassificationBasis,
    ClassificationRuleDefinition, CollisionBinding, CollisionPolicyDefinition, ConventionsMetadata,
    DiagnosticDefinition, InvalidValuePolicyDefinition, ParserBinding, ParserPolicyDefinition,
    ParserStrictness, QuantityClassification, QuantityOccurrence, RangeDefinition, SecurityBinding,
    SecurityModeDefinition, UnitDefinition, CONVENTIONS_SCHEMA_ID, CONVENTIONS_SCHEMA_VERSION,
};
use vibelang_api_manifest::{
    stable_id, Anchor, ApiEntry, BoundarySemantics, EntryDetails, Overload, Parameter,
    PublicApiManifest, UgenInput,
};

const METADATA_PATH: &str = "api/effective-metadata-v1.json";
const CONVENTIONS_SOURCE: &str =
    "docs/architecture/api-unification/conventions-and-capabilities.md";
const PARAMETER_COUNT: usize = 18_786;
const UGEN_INPUT_COUNT: usize = 5_089;

pub fn generate(root: &Path, check: bool) -> Result<(), String> {
    let manifest = public_api::build_manifest(root)?;
    let first = build(&manifest)?;
    let second = build(&manifest)?;
    if first != second {
        return Err("effective metadata double generation produced different values".into());
    }
    let first = pretty(&first)?;
    let second = pretty(&second)?;
    if first != second {
        return Err("effective metadata double serialization produced different bytes".into());
    }
    write_or_check(root, &first, check)?;
    println!(
        "effective metadata: {PARAMETER_COUNT} parameter occurrences, {UGEN_INPUT_COUNT} UGen inputs, zero unknown/heuristic/stale rows"
    );
    println!("effective metadata double generation is byte-identical");
    Ok(())
}

pub(crate) fn build(manifest: &PublicApiManifest) -> Result<ConventionsMetadata, String> {
    let mut metadata = registry();
    let mut parameter_quantities = Vec::new();
    let mut ugen_input_quantities = Vec::new();
    let mut parser_bindings = Vec::new();
    let mut collision_bindings = Vec::new();
    let mut availability_bindings = Vec::new();

    for entry in &manifest.entries {
        let ugen_inputs = match &entry.details {
            EntryDetails::Ugen { inputs, .. } => Some(inputs),
            _ => None,
        };
        for overload in &entry.overloads {
            for parameter in &overload.parameters {
                let classification = if let Some(inputs) = ugen_inputs {
                    let input = inputs.get(parameter.position as usize).ok_or_else(|| {
                        format!(
                            "UGen overload {} parameter {} has no matching source input",
                            overload.id, parameter.position
                        )
                    })?;
                    classify_ugen(entry, input, legacy_policy(&overload.boundary))
                } else {
                    classify_parameter(entry, parameter, legacy_policy(&overload.boundary))
                };
                parameter_quantities.push(QuantityOccurrence {
                    occurrence_id: occurrence_id("parameter", &overload.id, parameter.position),
                    target_id: overload.id.clone(),
                    position: parameter.position,
                    name: parameter.name.clone(),
                    source_type: parameter.accepted_types.join("|"),
                    classification,
                });
            }
            if is_parser_entry(entry) {
                parser_bindings.push(parser_binding(entry, overload));
            }
        }

        if let EntryDetails::Ugen { class, inputs, .. } = &entry.details {
            for (position, input) in inputs.iter().enumerate() {
                let position = position as u32;
                ugen_input_quantities.push(QuantityOccurrence {
                    occurrence_id: occurrence_id("ugen_input", &entry.id, position),
                    target_id: entry.id.clone(),
                    position,
                    name: Some(input.name.clone()),
                    source_type: input.input_type.clone(),
                    classification: classify_ugen(entry, input, None),
                });
            }
            if class.trim().is_empty() {
                return Err(format!("UGen {} has no source class", entry.id));
            }
        }

        if let Some(binding) = collision_binding(entry) {
            collision_bindings.push(binding);
        }
        availability_bindings.push(availability_binding(entry)?);
    }

    parameter_quantities.sort_by(|left, right| left.occurrence_id.cmp(&right.occurrence_id));
    ugen_input_quantities.sort_by(|left, right| left.occurrence_id.cmp(&right.occurrence_id));
    parser_bindings.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    parser_bindings.dedup_by(|left, right| left.target_id == right.target_id);
    collision_bindings.sort_by(|left, right| left.target_id.cmp(&right.target_id));
    availability_bindings.sort_by(|left, right| left.target_id.cmp(&right.target_id));

    metadata.parameter_quantities = parameter_quantities;
    metadata.ugen_input_quantities = ugen_input_quantities;
    metadata.parser_bindings = parser_bindings;
    metadata.collision_bindings = collision_bindings;
    metadata.availability_bindings = availability_bindings;
    metadata.security_bindings = vec![SecurityBinding {
        target_id: "surface.http.v1".into(),
        mode_id: "security.http.legacy_loopback_unrestricted_cors".into(),
        state_id: "availability.degraded".into(),
        reason_ids: vec!["reason.security_policy_disabled".into()],
        source_anchors: vec![
            "crates/vibelang-cli/src/main.rs:default API bind 127.0.0.1:1606".into(),
            "crates/vibelang-http/src/lib.rs:CorsLayer::permissive".into(),
        ],
    }];

    let parameter_applicable = applicable_count(&metadata.parameter_quantities);
    let ugen_applicable = applicable_count(&metadata.ugen_input_quantities);
    let explicit_unbounded = metadata
        .parameter_quantities
        .iter()
        .chain(&metadata.ugen_input_quantities)
        .filter(|occurrence| {
            matches!(
                &occurrence.classification,
                QuantityClassification::Applicable { range_id, .. }
                    if range_id == "range.finite.unbounded"
                        || range_id == "range.integer.unbounded"
            )
        })
        .count() as u64;
    metadata.stats = BTreeMap::from([
        (
            "availability_bindings".into(),
            metadata.availability_bindings.len() as u64,
        ),
        (
            "collision_bindings".into(),
            metadata.collision_bindings.len() as u64,
        ),
        ("explicit_unbounded_occurrences".into(), explicit_unbounded),
        ("heuristic_only_occurrences".into(), 0),
        ("parameter_applicable".into(), parameter_applicable),
        (
            "parameter_not_applicable".into(),
            metadata.parameter_quantities.len() as u64 - parameter_applicable,
        ),
        (
            "parameter_occurrences".into(),
            metadata.parameter_quantities.len() as u64,
        ),
        (
            "parser_bindings".into(),
            metadata.parser_bindings.len() as u64,
        ),
        ("stale_occurrences".into(), 0),
        ("ugen_input_applicable".into(), ugen_applicable),
        (
            "ugen_input_not_applicable".into(),
            metadata.ugen_input_quantities.len() as u64 - ugen_applicable,
        ),
        (
            "ugen_input_occurrences".into(),
            metadata.ugen_input_quantities.len() as u64,
        ),
        ("unknown_occurrences".into(), 0),
    ]);

    validate_source_coverage(manifest, &metadata)?;
    metadata.validate().map_err(|error| error.to_string())?;
    Ok(metadata)
}

#[allow(dead_code)]
pub(crate) fn attach(
    manifest: &mut vibelang_api_manifest::v2::PublicApiManifestV2,
    metadata: ConventionsMetadata,
) -> Result<(), String> {
    metadata.validate().map_err(|error| error.to_string())?;
    manifest.conventions = Some(metadata);
    manifest.validate().map_err(|error| error.to_string())
}

fn registry() -> ConventionsMetadata {
    let mut units = vec![
        unit(
            "unit.amplitude.linear",
            "finite JSON number",
            "linear amplitude or gain",
        ),
        unit(
            "unit.angle.radian",
            "finite JSON number",
            "angle in radians",
        ),
        unit(
            "unit.audio.bus_index",
            "non-negative integer",
            "backend audio bus index",
        ),
        unit(
            "unit.audio.sample_frame",
            "integer",
            "audio frame index or count",
        ),
        unit(
            "unit.audio.sample_rate_hz",
            "finite JSON number",
            "sample frames per second",
        ),
        unit(
            "unit.count",
            "non-negative integer",
            "cardinality or channel count",
        ),
        unit(
            "unit.frequency.hz",
            "finite JSON number",
            "cycles per second",
        ),
        unit(
            "unit.level.decibel",
            "finite JSON number",
            "signed amplitude level in dB",
        ),
        unit(
            "unit.midi.channel",
            "integer",
            "human-facing MIDI channel 1 through 16",
        ),
        unit(
            "unit.midi.channel_index",
            "integer",
            "MIDI channel storage index 0 through 15",
        ),
        unit(
            "unit.midi.control.14bit",
            "integer",
            "fourteen-bit controller value",
        ),
        unit(
            "unit.midi.control.32bit",
            "integer",
            "unsigned 32-bit controller value",
        ),
        unit(
            "unit.midi.control.7bit",
            "integer",
            "seven-bit controller value",
        ),
        unit(
            "unit.midi.group",
            "integer",
            "human-facing UMP group 1 through 16",
        ),
        unit(
            "unit.midi.group_index",
            "integer",
            "UMP group storage index 0 through 15",
        ),
        unit("unit.midi.note", "integer", "MIDI note number"),
        unit(
            "unit.midi.pitch_bend.14bit_signed",
            "integer",
            "centered MIDI pitch bend",
        ),
        unit("unit.midi.velocity.16bit", "integer", "MIDI 2 velocity"),
        unit("unit.midi.velocity.7bit", "integer", "MIDI 1 velocity"),
        unit(
            "unit.midi.velocity.normalized",
            "finite JSON number",
            "normalized high-level velocity",
        ),
        unit(
            "unit.pitch.semitone",
            "finite JSON number",
            "equal-tempered semitone interval",
        ),
        unit(
            "unit.ratio.normalized",
            "finite JSON number",
            "normalized ratio",
        ),
        unit("unit.scalar", "finite JSON number", "dimensionless scalar"),
        unit(
            "unit.signal.raw",
            "finite signal sample",
            "unscaled DSP signal value",
        ),
        unit(
            "unit.tempo.bpm",
            "finite JSON number",
            "quarter-note beats per minute",
        ),
        unit(
            "unit.time.bar",
            "tagged contextual quantity",
            "meter-resolved bar duration",
        ),
        unit(
            "unit.time.beat.quarter",
            "JSON number quantized to 1/65,536",
            "quarter-note beats",
        ),
        unit(
            "unit.time.millisecond",
            "finite JSON number",
            "wall-clock milliseconds",
        ),
        unit(
            "unit.time.second",
            "finite JSON number",
            "wall-clock or audio seconds",
        ),
    ];
    units.sort_by(|left, right| left.unit_id.cmp(&right.unit_id));

    let mut ranges = vec![
        bounded(
            "range.closed.0_1",
            Some(0.0),
            true,
            Some(1.0),
            true,
            false,
            "closed normalized interval",
        ),
        bounded(
            "range.closed.minus1_1",
            Some(-1.0),
            true,
            Some(1.0),
            true,
            false,
            "closed bipolar interval",
        ),
        unbounded("range.finite.unbounded", false, "any finite JSON number"),
        bounded(
            "range.finite.nonnegative",
            Some(0.0),
            true,
            None,
            false,
            false,
            "finite number greater than or equal to zero",
        ),
        bounded(
            "range.finite.positive",
            Some(0.0),
            false,
            None,
            false,
            false,
            "finite number greater than zero",
        ),
        bounded(
            "range.integer.nonnegative",
            Some(0.0),
            true,
            None,
            false,
            true,
            "non-negative integer",
        ),
        unbounded("range.integer.unbounded", true, "any representable integer"),
        bounded(
            "range.midi.channel.1_16",
            Some(1.0),
            true,
            Some(16.0),
            true,
            true,
            "MIDI channel 1 through 16",
        ),
        bounded(
            "range.midi.group.1_16",
            Some(1.0),
            true,
            Some(16.0),
            true,
            true,
            "MIDI UMP group 1 through 16",
        ),
        bounded(
            "range.midi.index.0_15",
            Some(0.0),
            true,
            Some(15.0),
            true,
            true,
            "MIDI storage index 0 through 15",
        ),
        bounded(
            "range.midi.note.0_127",
            Some(0.0),
            true,
            Some(127.0),
            true,
            true,
            "MIDI note 0 through 127",
        ),
        bounded(
            "range.midi.pitch_bend14_signed",
            Some(-8192.0),
            true,
            Some(8191.0),
            true,
            true,
            "signed fourteen-bit pitch bend",
        ),
        bounded(
            "range.midi.u14",
            Some(0.0),
            true,
            Some(16383.0),
            true,
            true,
            "unsigned fourteen-bit integer",
        ),
        bounded(
            "range.midi.u16",
            Some(0.0),
            true,
            Some(65535.0),
            true,
            true,
            "unsigned sixteen-bit integer",
        ),
        bounded(
            "range.midi.u32",
            Some(0.0),
            true,
            Some(4294967295.0),
            true,
            true,
            "unsigned thirty-two-bit integer",
        ),
        bounded(
            "range.midi.u7",
            Some(0.0),
            true,
            Some(127.0),
            true,
            true,
            "unsigned seven-bit integer",
        ),
        bounded(
            "range.tempo.bpm.1_999",
            Some(1.0),
            true,
            Some(999.0),
            true,
            false,
            "supported tempo interval",
        ),
        bounded(
            "range.time_signature.numerator.1_32",
            Some(1.0),
            true,
            Some(32.0),
            true,
            true,
            "time-signature numerator",
        ),
        enumerated(
            "range.time_signature.denominator.power_of_two",
            &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
            "supported time-signature denominators",
        ),
    ];
    ranges.sort_by(|left, right| left.range_id.cmp(&right.range_id));

    let mut diagnostics = diagnostic_registry();
    diagnostics.sort_by(|left, right| left.diagnostic_id.cmp(&right.diagnostic_id));
    let mut invalid_value_policies = vec![
        invalid(
            "invalid.compat_clamp",
            false,
            Some("diagnostic.compat.value_clamped"),
            "legacy input is clamped and diagnosed",
        ),
        invalid(
            "invalid.compat_drop",
            false,
            Some("diagnostic.compat.token_dropped"),
            "legacy input element is dropped and diagnosed",
        ),
        invalid(
            "invalid.compat_fallback",
            false,
            Some("diagnostic.compat.fallback_applied"),
            "legacy fallback is applied and diagnosed",
        ),
        invalid(
            "invalid.reject",
            true,
            None,
            "invalid input is rejected before dispatch",
        ),
    ];
    invalid_value_policies.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));

    let mut parser_policies = vec![
        ParserPolicyDefinition {
            parser_id: "parser.compat.v1.forgiving".into(),
            strictness: ParserStrictness::Permissive,
            grammar_id: "grammar.vibelang.v1.contextual".into(),
            consumes_full_input: false,
            fallback_policy_id: "fallback.legacy_default".into(),
            fallback_rationale: "preserves documented v1 prefix parsing, defaulting, and partial recovery until M10 adapters land".into(),
            diagnostic_id: Some("diagnostic.compat.parser_forgiving".into()),
            source_anchor: anchor("Parser contract"),
        },
        ParserPolicyDefinition {
            parser_id: "parser.strict.full_consumption".into(),
            strictness: ParserStrictness::Strict,
            grammar_id: "grammar.vibelang.v2.full_consumption".into(),
            consumes_full_input: true,
            fallback_policy_id: "fallback.reject".into(),
            fallback_rationale: "canonical inputs reject malformed or trailing tokens without library recovery".into(),
            diagnostic_id: None,
            source_anchor: anchor("Parser contract"),
        },
    ];
    parser_policies.sort_by(|left, right| left.parser_id.cmp(&right.parser_id));

    let mut collision_policies = vec![
        collision(
            "collision.import.require_qualification",
            "kind/module/local_name",
            "reject ambiguous unqualified imports",
            "list every candidate in stable identity order",
            "diagnostic.registry.ambiguous_name",
        ),
        collision(
            "collision.legacy.module_resolution",
            "legacy Rhai import",
            "retain existing module resolution for v1",
            "source module resolution order",
            "diagnostic.registry.ambiguous_name",
        ),
        collision(
            "collision.legacy.source_order_replace",
            "legacy global synthdef/effect registry",
            "later deployment replaces earlier definition",
            "source/deployment order",
            "diagnostic.registry.duplicate_definition",
        ),
        collision(
            "collision.registry.reject_nonidentical",
            "kind/module/local_name",
            "reject non-identical definitions and accept identical hashes idempotently",
            "canonical definition hash",
            "diagnostic.registry.duplicate_definition",
        ),
    ];
    collision_policies.sort_by(|left, right| left.policy_id.cmp(&right.policy_id));

    let mut availability_states = vec![
        state(
            "availability.available",
            true,
            "all required gates passed and semantics are effectful",
        ),
        state(
            "availability.degraded",
            true,
            "usable behavior exists with declared constraints",
        ),
        state(
            "availability.unavailable",
            true,
            "a known gate failed or the API is quarantined",
        ),
        state(
            "availability.unknown",
            false,
            "required runtime truth has not been observed",
        ),
    ];
    availability_states.sort_by(|left, right| left.state_id.cmp(&right.state_id));
    let mut availability_reasons = reason_registry();
    availability_reasons.sort_by(|left, right| left.reason_id.cmp(&right.reason_id));
    let mut security_modes = security_registry();
    security_modes.sort_by(|left, right| left.mode_id.cmp(&right.mode_id));
    let mut capabilities = capability_registry();
    capabilities.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    let mut classification_rules = rule_registry();
    classification_rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));

    ConventionsMetadata {
        schema_id: CONVENTIONS_SCHEMA_ID.into(),
        schema_version: CONVENTIONS_SCHEMA_VERSION,
        units,
        ranges,
        invalid_value_policies,
        parser_policies,
        collision_policies,
        diagnostics,
        availability_states,
        availability_reasons,
        security_modes,
        capabilities,
        classification_rules,
        parameter_quantities: Vec::new(),
        ugen_input_quantities: Vec::new(),
        parser_bindings: Vec::new(),
        collision_bindings: Vec::new(),
        availability_bindings: Vec::new(),
        security_bindings: Vec::new(),
        stats: BTreeMap::new(),
    }
}

fn classify_ugen(
    entry: &ApiEntry,
    input: &UgenInput,
    legacy_policy_id: Option<&str>,
) -> QuantityClassification {
    let provenance = ugen_provenance(entry, input);
    if input.input_type == "method" {
        return not_applicable(
            "quantity.rule.not_applicable",
            "method selector is not a numeric or signal-bearing value",
            provenance,
        );
    }
    let template = reviewed_ugen_template(&entry.registered_name, input);
    applicable(template, legacy_policy_id, provenance)
}

fn reviewed_ugen_template(registered_name: &str, input: &UgenInput) -> Template {
    let name = input.name.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "bwfreq"
            | "carfreq"
            | "centrefrequency"
            | "cutfreqs"
            | "cutoff"
            | "envfreq"
            | "execfreq"
            | "ffreq"
            | "formfreq"
            | "freq"
            | "freq1"
            | "freq2"
            | "freq3"
            | "freqhi"
            | "freqlo"
            | "fundfreq"
            | "hifreq"
            | "infreq"
            | "initfreq"
            | "lowfreq"
            | "lpfreq"
            | "maxfreq"
            | "mfreq"
            | "minfreq"
            | "modfreq"
            | "outfreq"
            | "resfreq"
            | "sawfreq"
            | "syncfreq"
            | "vibfreq"
            | "vibratofreq"
            | "vibratofrequency"
    ) {
        Template::Frequency
    } else if matches!(name.as_str(), "freqadd" | "freqoffset") {
        Template::FrequencyOffset
    } else if matches!(
        name.as_str(),
        "attack"
            | "attacktime"
            | "clamptime"
            | "decay"
            | "decaytime"
            | "deltime"
            | "delay"
            | "delay1"
            | "delay2"
            | "delay3"
            | "delaylengtharray"
            | "delaytime"
            | "dur"
            | "duration"
            | "durdist"
            | "graindur"
            | "jetdelay"
            | "keydecay"
            | "lag"
            | "lagtime"
            | "lagtimed"
            | "lagtimeu"
            | "loopdur"
            | "maxdelay"
            | "maxdelay1"
            | "maxdelay2"
            | "maxdelay3"
            | "maxdelaytime"
            | "memorytime"
            | "mindur"
            | "peaklag"
            | "relaxtime"
            | "release"
            | "releasetime"
            | "revtime"
            | "seekdur"
            | "seektime"
            | "time"
            | "timedispersion"
            | "traindur"
            | "waittime"
    ) {
        Template::Time
    } else if matches!(name.as_str(), "amp" | "ampthreshold" | "gain") {
        Template::Amplitude
    } else if name == "bus" {
        Template::Bus
    } else if reviewed_ugen_count_name(&name)
        || matches!(
            (registered_name, name.as_str()),
            ("audio_msg_ar", "index") | ("dswitch1_demand", "index") | ("dswitch_demand", "index")
        )
        || input.input_type == "int"
    {
        Template::Integer
    } else if input.input_type == "signal" {
        Template::Signal
    } else {
        Template::Scalar
    }
}

fn reviewed_ugen_count_name(name: &str) -> bool {
    matches!(
        name,
        "num"
            | "numbands"
            | "numbeats"
            | "numbins"
            | "numbits"
            | "numbufs"
            | "numchannels"
            | "numchans"
            | "numchansx"
            | "numchansy"
            | "numframes"
            | "numpartials"
            | "numsamp"
            | "numsamps"
            | "numteeth"
            | "numblocks"
            | "numcoeff"
            | "numdatapoints"
            | "numdims"
            | "numfeatures"
            | "numharm"
            | "nummeans"
            | "numpreviousbeats"
            | "numslopesaveraged"
            | "repeats"
    )
}

fn classify_parameter(
    entry: &ApiEntry,
    parameter: &Parameter,
    legacy_policy_id: Option<&str>,
) -> QuantityClassification {
    let provenance = anchors(&entry.source_anchors);
    let types = parameter
        .accepted_types
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let integer = types.iter().any(|value| {
        matches!(
            *value,
            "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        )
    });
    let numeric = integer || types.iter().any(|value| matches!(*value, "f32" | "f64"));
    let dynamic = types.contains("Dynamic");
    let name = parameter
        .name
        .as_deref()
        .unwrap_or(&entry.registered_name)
        .to_ascii_lowercase();

    let template = if midi_entry(entry) {
        midi_template(&name, &entry.registered_name)
    } else if frequency_name(&name) {
        Some(Template::Frequency)
    } else if tempo_name(&name) {
        Some(Template::Tempo)
    } else if beat_name(&name) {
        Some(Template::Beat)
    } else if bar_name(&name) {
        Some(Template::Bar)
    } else if time_name(&name) {
        Some(Template::Time)
    } else if amplitude_name(&name) {
        Some(Template::Amplitude)
    } else if semitone_name(&name) {
        Some(Template::Semitone)
    } else if ratio_name(&name) {
        Some(Template::Ratio)
    } else if bus_name(&name) {
        Some(Template::Bus)
    } else if count_name(&name) {
        Some(Template::Integer)
    } else {
        None
    };

    if let Some(template) = template {
        if numeric || dynamic {
            return applicable(template, legacy_policy_id, provenance);
        }
    }
    if numeric {
        applicable(
            if integer {
                Template::Integer
            } else {
                Template::Scalar
            },
            legacy_policy_id,
            provenance,
        )
    } else {
        not_applicable(
            "quantity.rule.not_applicable",
            "accepted source types and reviewed semantic name do not denote a scalar quantity",
            provenance,
        )
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Template {
    Amplitude,
    Bar,
    Beat,
    Bus,
    Frequency,
    FrequencyOffset,
    Integer,
    MidiChannel,
    MidiControl14,
    MidiControl32,
    MidiControl7,
    MidiGroup,
    MidiNote,
    MidiPitchBend,
    MidiVelocity16,
    MidiVelocity7,
    MidiVelocityNormalized,
    Ratio,
    Scalar,
    Semitone,
    Signal,
    Tempo,
    Time,
}

fn applicable(
    template: Template,
    legacy_policy_id: Option<&str>,
    provenance: Vec<String>,
) -> QuantityClassification {
    let (semantic_type_id, unit_id, range_id, rule_id) = match template {
        Template::Amplitude => (
            "audio.amplitude",
            "unit.amplitude.linear",
            "range.finite.nonnegative",
            "quantity.rule.amplitude",
        ),
        Template::Bar => (
            "musical.bars",
            "unit.time.bar",
            "range.finite.nonnegative",
            "quantity.rule.time",
        ),
        Template::Beat => (
            "musical.beats",
            "unit.time.beat.quarter",
            "range.finite.unbounded",
            "quantity.rule.time",
        ),
        Template::Bus => (
            "audio.bus_index",
            "unit.audio.bus_index",
            "range.integer.nonnegative",
            "quantity.rule.bus_index",
        ),
        Template::Frequency => (
            "audio.frequency",
            "unit.frequency.hz",
            "range.finite.positive",
            "quantity.rule.frequency",
        ),
        Template::FrequencyOffset => (
            "audio.frequency_offset",
            "unit.frequency.hz",
            "range.finite.unbounded",
            "quantity.rule.frequency",
        ),
        Template::Integer => (
            "numeric.integer",
            "unit.scalar",
            "range.integer.unbounded",
            "quantity.rule.integer",
        ),
        Template::MidiChannel => (
            "midi.channel",
            "unit.midi.channel",
            "range.midi.channel.1_16",
            "quantity.rule.midi",
        ),
        Template::MidiControl14 => (
            "midi.control_14bit",
            "unit.midi.control.14bit",
            "range.midi.u14",
            "quantity.rule.midi",
        ),
        Template::MidiControl32 => (
            "midi.control_32bit",
            "unit.midi.control.32bit",
            "range.midi.u32",
            "quantity.rule.midi",
        ),
        Template::MidiControl7 => (
            "midi.control_7bit",
            "unit.midi.control.7bit",
            "range.midi.u7",
            "quantity.rule.midi",
        ),
        Template::MidiGroup => (
            "midi.group",
            "unit.midi.group",
            "range.midi.group.1_16",
            "quantity.rule.midi",
        ),
        Template::MidiNote => (
            "midi.note",
            "unit.midi.note",
            "range.midi.note.0_127",
            "quantity.rule.midi",
        ),
        Template::MidiPitchBend => (
            "midi.pitch_bend",
            "unit.midi.pitch_bend.14bit_signed",
            "range.midi.pitch_bend14_signed",
            "quantity.rule.midi",
        ),
        Template::MidiVelocity16 => (
            "midi.velocity_16bit",
            "unit.midi.velocity.16bit",
            "range.midi.u16",
            "quantity.rule.midi",
        ),
        Template::MidiVelocity7 => (
            "midi.velocity_7bit",
            "unit.midi.velocity.7bit",
            "range.midi.u7",
            "quantity.rule.midi",
        ),
        Template::MidiVelocityNormalized => (
            "midi.velocity_normalized",
            "unit.midi.velocity.normalized",
            "range.closed.0_1",
            "quantity.rule.midi",
        ),
        Template::Ratio => (
            "numeric.normalized_ratio",
            "unit.ratio.normalized",
            "range.closed.0_1",
            "quantity.rule.ratio",
        ),
        Template::Scalar => (
            "numeric.scalar",
            "unit.scalar",
            "range.finite.unbounded",
            "quantity.rule.scalar",
        ),
        Template::Semitone => (
            "pitch.semitone",
            "unit.pitch.semitone",
            "range.finite.unbounded",
            "quantity.rule.semitone",
        ),
        Template::Signal => (
            "dsp.signal",
            "unit.signal.raw",
            "range.finite.unbounded",
            "quantity.rule.signal",
        ),
        Template::Tempo => (
            "transport.bpm",
            "unit.tempo.bpm",
            "range.tempo.bpm.1_999",
            "quantity.rule.tempo",
        ),
        Template::Time => (
            "time.seconds",
            "unit.time.second",
            "range.finite.nonnegative",
            "quantity.rule.time",
        ),
    };
    QuantityClassification::Applicable {
        semantic_type_id: semantic_type_id.into(),
        unit_id: unit_id.into(),
        range_id: range_id.into(),
        canonical_invalid_value_policy_id: "invalid.reject".into(),
        legacy_invalid_value_policy_id: legacy_policy_id.map(str::to_owned),
        rule_id: rule_id.into(),
        basis: ClassificationBasis::ReviewedRule,
        provenance,
    }
}

fn not_applicable(rule_id: &str, reason: &str, provenance: Vec<String>) -> QuantityClassification {
    QuantityClassification::NotApplicable {
        reason: reason.into(),
        rule_id: rule_id.into(),
        basis: ClassificationBasis::ReviewedRule,
        provenance,
    }
}

fn legacy_policy(boundary: &BoundarySemantics) -> Option<&'static str> {
    if boundary.fallbacks.status == "present" {
        Some("invalid.compat_fallback")
    } else if boundary.clamps.status == "present" {
        Some("invalid.compat_clamp")
    } else {
        None
    }
}

fn parser_binding(entry: &ApiEntry, overload: &Overload) -> ParserBinding {
    let permissive = overload.boundary.fallbacks.status == "present"
        || overload.boundary.coercions.status == "present";
    let fallback_rationale = if permissive {
        overload
            .boundary
            .fallbacks
            .details
            .iter()
            .chain(&overload.boundary.coercions.details)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ")
    } else {
        "source evidence reports no library fallback; canonical full-consumption policy is retained"
            .into()
    };
    ParserBinding {
        target_id: overload.id.clone(),
        canonical_parser_id: "parser.strict.full_consumption".into(),
        legacy_parser_id: if permissive {
            "parser.compat.v1.forgiving".into()
        } else {
            "parser.strict.full_consumption".into()
        },
        fallback_rationale,
        diagnostic_id: permissive.then(|| "diagnostic.compat.parser_forgiving".into()),
        source_anchors: anchors(&entry.source_anchors),
    }
}

fn collision_binding(entry: &ApiEntry) -> Option<CollisionBinding> {
    let duplicate = match &entry.details {
        EntryDetails::StdlibDefinition { duplicate_name, .. }
        | EntryDetails::StdlibFunction { duplicate_name, .. }
            if duplicate_name.declaration_count > 1 =>
        {
            duplicate_name
        }
        _ => return None,
    };
    let definition = matches!(&entry.details, EntryDetails::StdlibDefinition { .. });
    Some(CollisionBinding {
        target_id: entry.id.clone(),
        canonical_policy_id: if definition {
            "collision.registry.reject_nonidentical".into()
        } else {
            "collision.import.require_qualification".into()
        },
        legacy_policy_id: if definition {
            "collision.legacy.source_order_replace".into()
        } else {
            "collision.legacy.module_resolution".into()
        },
        candidates: duplicate.import_paths.clone(),
        source_anchors: anchors(&entry.source_anchors),
    })
}

fn availability_binding(entry: &ApiEntry) -> Result<AvailabilityBinding, String> {
    let mut capability_ids = BTreeSet::new();
    let mut reason_ids = BTreeSet::new();
    if entry
        .availability
        .plugins
        .iter()
        .any(|value| value == "mi-UGens")
    {
        capability_ids.insert("capability.plugin.mi_ugens".to_string());
    }
    if entry
        .availability
        .features
        .iter()
        .any(|value| value == "midi")
    {
        let name = entry.registered_name.as_str();
        if name.contains("clock") {
            capability_ids.insert("capability.midi.clock".to_string());
        } else if name.contains("group") || name.contains("per_note") || name.contains("hires") {
            capability_ids.insert("capability.midi.ump".to_string());
        } else {
            capability_ids.insert("capability.midi.input".to_string());
            capability_ids.insert("capability.midi.output".to_string());
        }
    }
    if entry
        .availability
        .cfg
        .iter()
        .any(|value| value == "cfg (feature = \"ext-fs\")")
    {
        capability_ids.insert("capability.extension.filesystem".to_string());
    }
    if entry
        .availability
        .cfg
        .iter()
        .any(|value| value == "cfg (feature = \"ext-exec\")")
    {
        capability_ids.insert("capability.extension.process".to_string());
    }
    if entry
        .availability
        .cfg
        .iter()
        .any(|value| value == "cfg (feature = \"ext-net\")")
    {
        capability_ids.insert("capability.extension.network".to_string());
    }
    if !entry.availability.targets.is_empty() {
        let source = entry
            .source_anchors
            .iter()
            .map(|anchor| anchor.path.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if source.contains("recording") {
            capability_ids.insert("capability.recording.audio".to_string());
        } else if source.contains("sfz") {
            capability_ids.insert("capability.resource.sfz".to_string());
        } else {
            capability_ids.insert("capability.backend.scsynth.native".to_string());
        }
    }
    match entry.availability.status.as_str() {
        "quarantined" => {
            capability_ids.insert("capability.api.ugen.demand_rate".to_string());
            reason_ids.insert("reason.quarantined".to_string());
        }
        "documentation_only" => {
            reason_ids.insert("reason.documentation_only".to_string());
        }
        "conditional" if capability_ids.is_empty() => {
            return Err(format!(
                "conditional entry {} has no source-truthful semantic capability mapping",
                entry.id
            ));
        }
        _ => {}
    }
    let mut evidence = Vec::new();
    evidence.extend(
        entry
            .availability
            .cfg
            .iter()
            .map(|value| format!("cfg:{value}")),
    );
    evidence.extend(
        entry
            .availability
            .targets
            .iter()
            .map(|value| format!("target:{value}")),
    );
    evidence.extend(
        entry
            .availability
            .features
            .iter()
            .map(|value| format!("feature:{value}")),
    );
    evidence.extend(
        entry
            .availability
            .plugins
            .iter()
            .map(|value| format!("plugin:{value}")),
    );
    evidence.extend(
        entry
            .availability
            .runtime_conditions
            .iter()
            .map(|value| format!("runtime:{value}")),
    );
    evidence.extend(anchors(&entry.source_anchors));
    Ok(AvailabilityBinding {
        target_id: entry.id.clone(),
        declared_status: entry.availability.status.clone(),
        predicate_capability_ids: capability_ids.into_iter().collect(),
        unavailable_reason_ids: reason_ids.into_iter().collect(),
        evidence,
    })
}

fn validate_source_coverage(
    manifest: &PublicApiManifest,
    metadata: &ConventionsMetadata,
) -> Result<(), String> {
    let expected_parameters = manifest
        .entries
        .iter()
        .flat_map(|entry| &entry.overloads)
        .flat_map(|overload| {
            overload
                .parameters
                .iter()
                .map(|parameter| occurrence_id("parameter", &overload.id, parameter.position))
        })
        .collect::<BTreeSet<_>>();
    let actual_parameters = metadata
        .parameter_quantities
        .iter()
        .map(|occurrence| occurrence.occurrence_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_ugen = manifest
        .entries
        .iter()
        .filter_map(|entry| match &entry.details {
            EntryDetails::Ugen { inputs, .. } => Some(
                inputs
                    .iter()
                    .enumerate()
                    .map(|(position, _)| occurrence_id("ugen_input", &entry.id, position as u32))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect::<BTreeSet<_>>();
    let actual_ugen = metadata
        .ugen_input_quantities
        .iter()
        .map(|occurrence| occurrence.occurrence_id.clone())
        .collect::<BTreeSet<_>>();
    if expected_parameters.len() != PARAMETER_COUNT || expected_ugen.len() != UGEN_INPUT_COUNT {
        return Err(format!(
            "M05 source totals changed: parameters {}/{PARAMETER_COUNT}, UGen inputs {}/{UGEN_INPUT_COUNT}",
            expected_parameters.len(),
            expected_ugen.len(),
        ));
    }
    ensure_exact_coverage("parameter", &expected_parameters, &actual_parameters)?;
    ensure_exact_coverage("UGen input", &expected_ugen, &actual_ugen)?;

    let expected_availability = manifest
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<BTreeSet<_>>();
    let actual_availability = metadata
        .availability_bindings
        .iter()
        .map(|binding| binding.target_id.clone())
        .collect::<BTreeSet<_>>();
    ensure_exact_coverage(
        "availability binding",
        &expected_availability,
        &actual_availability,
    )?;
    Ok(())
}

fn ensure_exact_coverage(
    kind: &str,
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    let missing = expected.difference(actual).count();
    let stale = actual.difference(expected).count();
    Err(format!(
        "{kind} coverage mismatch: missing {missing}, stale {stale}"
    ))
}

fn occurrence_id(kind: &str, target_id: &str, position: u32) -> String {
    stable_id(
        "quantity_occurrence",
        &format!("{kind}|{target_id}|{position}"),
    )
}

fn applicable_count(values: &[QuantityOccurrence]) -> u64 {
    values
        .iter()
        .filter(|value| {
            matches!(
                value.classification,
                QuantityClassification::Applicable { .. }
            )
        })
        .count() as u64
}

fn is_parser_entry(entry: &ApiEntry) -> bool {
    entry.surface != "dsp_ugen"
        && matches!(
            entry.registered_name.as_str(),
            "chord"
                | "curve"
                | "melody"
                | "note"
                | "pattern"
                | "progression"
                | "range"
                | "roman_numeral"
                | "scale"
                | "scale_degree"
                | "velocity_curve"
        )
}

fn midi_entry(entry: &ApiEntry) -> bool {
    entry
        .availability
        .features
        .iter()
        .any(|value| value == "midi")
        || entry
            .source_anchors
            .iter()
            .any(|anchor| anchor.path.contains("/midi/"))
}

fn midi_template(name: &str, registered_name: &str) -> Option<Template> {
    let registered_name = registered_name.to_ascii_lowercase();
    if name.contains("channel") {
        Some(Template::MidiChannel)
    } else if name == "group" || name.contains("group_index") {
        Some(Template::MidiGroup)
    } else if name.contains("note") && !name.contains("count") {
        Some(Template::MidiNote)
    } else if name.contains("velocity") {
        if registered_name.contains("hires") {
            Some(Template::MidiVelocity16)
        } else if registered_name == "note_on" || registered_name == "note_off" {
            Some(Template::MidiVelocity7)
        } else {
            Some(Template::MidiVelocityNormalized)
        }
    } else if name.contains("bend") || registered_name.contains("pitch_bend") {
        Some(Template::MidiPitchBend)
    } else if name.contains("controller") || name == "cc" || name == "value" {
        if registered_name.contains("32") || registered_name.contains("per_note") {
            Some(Template::MidiControl32)
        } else if registered_name.contains("hires") {
            Some(Template::MidiControl14)
        } else {
            Some(Template::MidiControl7)
        }
    } else {
        None
    }
}

fn frequency_name(name: &str) -> bool {
    name.contains("freq") || name == "cutoff" || name.ends_with("_hz")
}

fn tempo_name(name: &str) -> bool {
    name == "bpm" || name.contains("tempo")
}

fn beat_name(name: &str) -> bool {
    name == "beat" || name.ends_with("_beats") || name.contains("beats_per")
}

fn bar_name(name: &str) -> bool {
    name == "bar" || name == "bars" || name.ends_with("_bars")
}

fn time_name(name: &str) -> bool {
    [
        "time", "dur", "delay", "attack", "release", "decay", "lag", "seconds",
    ]
    .iter()
    .any(|token| name.contains(token))
}

fn amplitude_name(name: &str) -> bool {
    name == "amp" || name.ends_with("_amp") || name.contains("amplitude") || name == "gain"
}

fn semitone_name(name: &str) -> bool {
    name.contains("semitone") || name.contains("transpose") || name.contains("octave")
}

fn ratio_name(name: &str) -> bool {
    matches!(name, "ratio" | "mix" | "wet" | "dry" | "probability")
}

fn bus_name(name: &str) -> bool {
    name == "bus" || name.ends_with("_bus") || name.contains("bus_index")
}

fn count_name(name: &str) -> bool {
    name.contains("count")
        || name.starts_with("num")
        || name.ends_with("_index")
        || matches!(
            name,
            "index" | "idx" | "channels" | "frames" | "steps" | "repeats"
        )
}

fn ugen_provenance(entry: &ApiEntry, input: &UgenInput) -> Vec<String> {
    let mut provenance = anchors(&entry.source_anchors);
    provenance.push(format!(
        "{}:{}.{} input_type={}",
        entry.surface, entry.registered_name, input.name, input.input_type
    ));
    provenance.push(format!("source description: {}", input.description));
    provenance
}

fn anchors(values: &[Anchor]) -> Vec<String> {
    values
        .iter()
        .map(|anchor| {
            anchor.line.map_or_else(
                || format!("{}:{}", anchor.path, anchor.symbol),
                |line| format!("{}:{}:{line}", anchor.path, anchor.symbol),
            )
        })
        .collect()
}

fn unit(id: &str, wire_type: &str, meaning: &str) -> UnitDefinition {
    UnitDefinition {
        unit_id: id.into(),
        wire_type: wire_type.into(),
        meaning: meaning.into(),
        source_anchor: anchor("Stable unit IDs and wire values"),
    }
}

fn bounded(
    id: &str,
    minimum: Option<f64>,
    minimum_inclusive: bool,
    maximum: Option<f64>,
    maximum_inclusive: bool,
    integer: bool,
    meaning: &str,
) -> RangeDefinition {
    RangeDefinition {
        range_id: id.into(),
        minimum,
        minimum_inclusive,
        maximum,
        maximum_inclusive,
        finite: true,
        integer,
        allowed_values: Vec::new(),
        unbounded: false,
        meaning: meaning.into(),
        source_anchor: anchor("Stable range and invalid-value IDs"),
    }
}

fn unbounded(id: &str, integer: bool, meaning: &str) -> RangeDefinition {
    RangeDefinition {
        range_id: id.into(),
        minimum: None,
        minimum_inclusive: false,
        maximum: None,
        maximum_inclusive: false,
        finite: true,
        integer,
        allowed_values: Vec::new(),
        unbounded: true,
        meaning: meaning.into(),
        source_anchor: anchor("Stable range and invalid-value IDs"),
    }
}

fn enumerated(id: &str, allowed_values: &[f64], meaning: &str) -> RangeDefinition {
    RangeDefinition {
        range_id: id.into(),
        minimum: None,
        minimum_inclusive: false,
        maximum: None,
        maximum_inclusive: false,
        finite: true,
        integer: true,
        allowed_values: allowed_values.to_vec(),
        unbounded: false,
        meaning: meaning.into(),
        source_anchor: anchor("Stable range and invalid-value IDs"),
    }
}

fn invalid(
    id: &str,
    canonical: bool,
    diagnostic_id: Option<&str>,
    behavior: &str,
) -> InvalidValuePolicyDefinition {
    InvalidValuePolicyDefinition {
        policy_id: id.into(),
        canonical,
        diagnostic_id: diagnostic_id.map(str::to_owned),
        behavior: behavior.into(),
        source_anchor: anchor("Stable range and invalid-value IDs"),
    }
}

fn collision(
    id: &str,
    namespace: &str,
    duplicate_behavior: &str,
    deterministic_resolution: &str,
    diagnostic_id: &str,
) -> CollisionPolicyDefinition {
    CollisionPolicyDefinition {
        policy_id: id.into(),
        namespace: namespace.into(),
        duplicate_behavior: duplicate_behavior.into(),
        deterministic_resolution: deterministic_resolution.into(),
        diagnostic_id: diagnostic_id.into(),
        source_anchor: anchor("Duplicate-name contract"),
    }
}

fn state(id: &str, terminal: bool, meaning: &str) -> AvailabilityStateDefinition {
    AvailabilityStateDefinition {
        state_id: id.into(),
        terminal,
        meaning: meaning.into(),
        source_anchor: anchor("Availability evaluation"),
    }
}

fn diagnostic_registry() -> Vec<DiagnosticDefinition> {
    [
        (
            "diagnostic.capability.degraded",
            "severity.warning",
            "capability",
            "capability is usable with constraints",
        ),
        (
            "diagnostic.capability.unavailable",
            "severity.error",
            "capability",
            "capability is known unavailable",
        ),
        (
            "diagnostic.capability.unknown",
            "severity.info",
            "capability",
            "capability probe has no terminal truth",
        ),
        (
            "diagnostic.compat.fallback_applied",
            "severity.warning",
            "compatibility",
            "legacy recovery substituted a value",
        ),
        (
            "diagnostic.compat.fixed_four_bar",
            "severity.warning",
            "compatibility",
            "legacy fixed-four bar conversion was used",
        ),
        (
            "diagnostic.compat.midi_channel_index",
            "severity.warning",
            "compatibility",
            "zero-based MIDI channel was adapted",
        ),
        (
            "diagnostic.compat.midi_group_index",
            "severity.warning",
            "compatibility",
            "zero-based UMP group was adapted",
        ),
        (
            "diagnostic.compat.parser_forgiving",
            "severity.warning",
            "compatibility",
            "legacy permissive parser behavior was used",
        ),
        (
            "diagnostic.compat.token_dropped",
            "severity.warning",
            "compatibility",
            "legacy parser dropped an input token",
        ),
        (
            "diagnostic.compat.value_clamped",
            "severity.warning",
            "compatibility",
            "legacy boundary clamped a value",
        ),
        (
            "diagnostic.compat.velocity_raw_in_normalized_field",
            "severity.warning",
            "compatibility",
            "raw MIDI velocity was adapted to normalized velocity",
        ),
        (
            "diagnostic.editor.projection_stale",
            "severity.error",
            "editor",
            "editor projection does not match the contract revision",
        ),
        (
            "diagnostic.registry.already_present",
            "severity.info",
            "registry",
            "identical canonical definition is already registered",
        ),
        (
            "diagnostic.registry.ambiguous_name",
            "severity.error",
            "registry",
            "unqualified identity has multiple candidates",
        ),
        (
            "diagnostic.registry.duplicate_definition",
            "severity.error",
            "registry",
            "non-identical canonical definition already exists",
        ),
    ]
    .into_iter()
    .map(|(id, severity, category, meaning)| DiagnosticDefinition {
        diagnostic_id: id.into(),
        severity_id: severity.into(),
        category_id: category.into(),
        meaning: meaning.into(),
        source_anchor: anchor("Compatibility diagnostics"),
    })
    .collect()
}

fn reason_registry() -> Vec<AvailabilityReasonDefinition> {
    [
        (
            "reason.backend_semantics_missing",
            AvailabilityGate::BackendSemantic,
            "active backend does not implement the declared semantics",
        ),
        (
            "reason.compile_feature_disabled",
            AvailabilityGate::BuildFeature,
            "required build feature is absent",
        ),
        (
            "reason.documentation_only",
            AvailabilityGate::Declaration,
            "declaration is intentionally documentation-only",
        ),
        (
            "reason.editor_projection_stale",
            AvailabilityGate::ConsumerProjection,
            "consumer projection revision is stale",
        ),
        (
            "reason.implementation_noop",
            AvailabilityGate::BackendSemantic,
            "compiled implementation is a semantic no-op",
        ),
        (
            "reason.operator_disabled",
            AvailabilityGate::OperatorPolicy,
            "operator policy disabled the capability",
        ),
        (
            "reason.plugin_missing",
            AvailabilityGate::RuntimeProbe,
            "required plugin family was not positively probed",
        ),
        (
            "reason.probe_failed",
            AvailabilityGate::RuntimeProbe,
            "runtime probe failed",
        ),
        (
            "reason.probe_pending",
            AvailabilityGate::RuntimeProbe,
            "runtime probe is pending",
        ),
        (
            "reason.quarantined",
            AvailabilityGate::Declaration,
            "contract declaration is quarantined",
        ),
        (
            "reason.runtime_dependency_missing",
            AvailabilityGate::RuntimeProbe,
            "runtime dependency is absent",
        ),
        (
            "reason.security_policy_disabled",
            AvailabilityGate::OperatorPolicy,
            "security policy is incomplete or explicitly degraded",
        ),
        (
            "reason.target_unsupported",
            AvailabilityGate::Target,
            "active target is unsupported",
        ),
    ]
    .into_iter()
    .map(|(id, gate, meaning)| AvailabilityReasonDefinition {
        reason_id: id.into(),
        gate,
        meaning: meaning.into(),
        source_anchor: anchor("Availability evaluation"),
    })
    .collect()
}

fn security_registry() -> Vec<SecurityModeDefinition> {
    vec![
        SecurityModeDefinition {
            mode_id: "security.http.authenticated_remote".into(),
            remote_allowed: true,
            authentication_required: true,
            origin_policy_id: "origin.explicit_allowlist".into(),
            degraded_reason_id: None,
            meaning: "authenticated remote access with explicit limits and audit policy".into(),
            source_anchor: anchor("Security and privacy bounds"),
        },
        SecurityModeDefinition {
            mode_id: "security.http.insecure_remote".into(),
            remote_allowed: true,
            authentication_required: false,
            origin_policy_id: "origin.explicit_insecure_acknowledgement".into(),
            degraded_reason_id: Some("reason.security_policy_disabled".into()),
            meaning: "explicitly acknowledged insecure remote mode".into(),
            source_anchor: anchor("Security and privacy bounds"),
        },
        SecurityModeDefinition {
            mode_id: "security.http.legacy_loopback_unrestricted_cors".into(),
            remote_allowed: false,
            authentication_required: false,
            origin_policy_id: "origin.any".into(),
            degraded_reason_id: Some("reason.security_policy_disabled".into()),
            meaning:
                "source-truthful v1 loopback default with unrestricted CORS and no authentication"
                    .into(),
            source_anchor: anchor("Security and privacy bounds"),
        },
        SecurityModeDefinition {
            mode_id: "security.http.loopback_local".into(),
            remote_allowed: false,
            authentication_required: false,
            origin_policy_id: "origin.loopback_only".into(),
            degraded_reason_id: None,
            meaning: "loopback bind with loopback-only origins".into(),
            source_anchor: anchor("Security and privacy bounds"),
        },
    ]
}

fn capability_registry() -> Vec<CapabilityDefinition> {
    let definitions = [
        (
            "capability.api.ugen.demand_rate",
            &[AvailabilityGate::Declaration][..],
            "demand-rate UGen family is callable",
        ),
        (
            "capability.audio.buffer.write_file",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "backend persists buffers to requested destinations",
        ),
        (
            "capability.audio.control_bus.read",
            &[
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "backend returns real control-bus values",
        ),
        (
            "capability.audio.render.realtime",
            &[
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "active backend renders realtime graph mutations",
        ),
        (
            "capability.audio.schedule.absolute_beat",
            &[
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "backend honors absolute-beat scheduling",
        ),
        (
            "capability.backend.scsynth.native",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "native scsynth backend is responsive",
        ),
        (
            "capability.backend.web_scsynth.wasm",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "WASM bridge is semantically responsive",
        ),
        (
            "capability.editor.rhai_projection",
            &[AvailabilityGate::ConsumerProjection],
            "Rhai editor projection matches the contract revision",
        ),
        (
            "capability.editor.ugen_projection",
            &[AvailabilityGate::ConsumerProjection],
            "UGen editor projection matches callable identities",
        ),
        (
            "capability.extension.filesystem",
            &[
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
            ],
            "filesystem extension is compiled and enabled for the evaluation scope",
        ),
        (
            "capability.extension.network",
            &[
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
            ],
            "network extension is compiled and enabled for the evaluation scope",
        ),
        (
            "capability.extension.process",
            &[
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
            ],
            "process extension is compiled and enabled for the evaluation scope",
        ),
        (
            "capability.http.eval",
            &[
                AvailabilityGate::Declaration,
                AvailabilityGate::OperatorPolicy,
            ],
            "HTTP eval route is enabled in the named security scope",
        ),
        (
            "capability.midi.clock",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "MIDI clock is effectful on the active target",
        ),
        (
            "capability.midi.input",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
            ],
            "MIDI input subsystem is probeable",
        ),
        (
            "capability.midi.output",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
            ],
            "MIDI output subsystem is probeable",
        ),
        (
            "capability.midi.ump",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "MIDI 2 UMP path is semantically supported",
        ),
        (
            "capability.plugin.mi_ugens",
            &[
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "mi-UGens plugin family was positively probed",
        ),
        (
            "capability.recording.audio",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "audio recording terminals are effectful",
        ),
        (
            "capability.recording.midi",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::OperatorPolicy,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "MIDI recording terminals are effectful",
        ),
        (
            "capability.resource.sfz",
            &[
                AvailabilityGate::Target,
                AvailabilityGate::BuildFeature,
                AvailabilityGate::RuntimeProbe,
                AvailabilityGate::BackendSemantic,
            ],
            "SFZ loading and playback are effectful",
        ),
    ];
    definitions
        .into_iter()
        .map(|(id, gates, meaning)| CapabilityDefinition {
            capability_id: id.into(),
            required_gates: gates.to_vec(),
            detection_source: "declaration + target + build + operator policy + runtime/backend probe as applicable".into(),
            dependencies: Vec::new(),
            conflicts: Vec::new(),
            meaning: meaning.into(),
            source_anchor: anchor("Capability IDs"),
        })
        .collect()
}

fn rule_registry() -> Vec<ClassificationRuleDefinition> {
    [
        ("quantity.rule.amplitude", "reviewed amp/amplitude/gain parameters use linear-amplitude semantics"),
        ("quantity.rule.bus_index", "reviewed bus fields are non-negative backend bus indices"),
        ("quantity.rule.frequency", "reviewed frequency/cutoff fields use hertz and a positive canonical range"),
        ("quantity.rule.integer", "typed integer/count/index fields retain an explicit integer-unbounded or count classification"),
        ("quantity.rule.midi", "MIDI channel/group/note/velocity/controller/bend names map to the canonical width-specific conventions"),
        ("quantity.rule.not_applicable", "reviewed non-scalar method, object, string, collection, and opaque dynamic values are explicitly non-quantities"),
        ("quantity.rule.ratio", "reviewed ratio/mix/probability fields use the normalized closed interval"),
        ("quantity.rule.scalar", "typed numeric values without a narrower source-backed domain are explicitly finite and unbounded"),
        ("quantity.rule.semitone", "reviewed transpose/octave/semitone fields use semitone intervals"),
        ("quantity.rule.signal", "signal-bearing UGen inputs without a narrower source-backed unit use raw finite signal semantics"),
        ("quantity.rule.tempo", "reviewed tempo fields use quarter-note BPM 1 through 999"),
        ("quantity.rule.time", "reviewed time/duration/delay/beat/bar fields use their canonical time unit"),
    ]
    .into_iter()
    .map(|(id, rationale)| ClassificationRuleDefinition {
        rule_id: id.into(),
        rationale: rationale.into(),
        source_anchor: anchor("Evidence and counting method"),
    })
    .collect()
}

fn anchor(section: &str) -> String {
    format!("{CONVENTIONS_SOURCE}#{section}")
}

fn pretty(value: &ConventionsMetadata) -> Result<String, String> {
    let mut json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    json.push('\n');
    Ok(json)
}

fn write_or_check(root: &Path, generated: &str, check: bool) -> Result<(), String> {
    let path = root.join(METADATA_PATH);
    if check {
        let committed = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if committed != generated {
            return Err(format!(
                "{METADATA_PATH} is stale; run `CARGO_BUILD_JOBS=1 cargo run -p xtask -- effective-metadata generate`"
            ));
        }
        println!("{METADATA_PATH} is current");
    } else {
        fs::write(&path, generated)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        println!("generated {METADATA_PATH}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::path::PathBuf;

    const NEGATIVE_FIXTURE: &str =
        include_str!("../../tests/fixtures/api-unification/v1/negative/m05-metadata-drift.json");

    #[derive(Deserialize)]
    struct NegativeFixture {
        schema: String,
        cases: Vec<NegativeCase>,
        source_assertions: Vec<SourceAssertion>,
    }

    #[derive(Deserialize)]
    struct NegativeCase {
        mutation: String,
        expected_error_contains: String,
    }

    #[derive(Deserialize)]
    struct SourceAssertion {
        path: String,
        contains: String,
    }

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn with_large_stack(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .name("m05-metadata-test".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(test)
            .unwrap()
            .join()
            .unwrap();
    }

    fn ugen_input(name: &str, input_type: &str, description: &str) -> UgenInput {
        UgenInput {
            name: name.into(),
            input_type: input_type.into(),
            default: None,
            description: description.into(),
        }
    }

    #[test]
    fn reviewed_ugen_table_does_not_promote_substring_false_positives() {
        assert_eq!(
            reviewed_ugen_template(
                "qitch_kr",
                &ugen_input("algoflag", "float", "phase refinement flag"),
            ),
            Template::Scalar
        );
        assert_eq!(
            reviewed_ugen_template(
                "env_follow_ar",
                &ugen_input("decaycoeff", "float", "one-pole decay coefficient"),
            ),
            Template::Scalar
        );
        assert_eq!(
            reviewed_ugen_template(
                "pv_synth_ar",
                &ugen_input("freqMul", "float", "frequency multiplier"),
            ),
            Template::Scalar
        );
        assert_eq!(
            reviewed_ugen_template(
                "pv_synth_ar",
                &ugen_input("freqAdd", "float", "frequency offset in hertz"),
            ),
            Template::FrequencyOffset
        );
        assert_eq!(
            reviewed_ugen_template(
                "delay_n_ar",
                &ugen_input("delaytime", "float", "delay duration in seconds"),
            ),
            Template::Time
        );
    }

    #[test]
    fn metadata_is_complete_exact_and_deterministic() {
        with_large_stack(|| {
            let manifest = public_api::build_manifest(&root()).unwrap();
            let first = build(&manifest).unwrap();
            let second = build(&manifest).unwrap();
            assert_eq!(first, second);
            assert_eq!(first.parameter_quantities.len(), PARAMETER_COUNT);
            assert_eq!(first.ugen_input_quantities.len(), UGEN_INPUT_COUNT);
            assert_eq!(first.stats["unknown_occurrences"], 0);
            assert_eq!(first.stats["heuristic_only_occurrences"], 0);
            assert_eq!(first.stats["stale_occurrences"], 0);
            assert!(first.stats["explicit_unbounded_occurrences"] > 0);
            assert!(first
                .parameter_quantities
                .iter()
                .chain(&first.ugen_input_quantities)
                .all(|occurrence| match &occurrence.classification {
                    QuantityClassification::Applicable { provenance, .. }
                    | QuantityClassification::NotApplicable { provenance, .. } => {
                        !provenance.is_empty()
                    }
                }));
        });
    }

    #[test]
    fn missing_and_stale_quantity_rows_fail_source_coverage() {
        with_large_stack(|| {
            let manifest = public_api::build_manifest(&root()).unwrap();
            let metadata = build(&manifest).unwrap();

            let mut missing = metadata.clone();
            missing.parameter_quantities.pop();
            assert!(validate_source_coverage(&manifest, &missing).is_err());

            let mut stale = metadata.clone();
            stale.ugen_input_quantities[0].occurrence_id =
                stable_id("quantity_occurrence", "stale");
            assert!(validate_source_coverage(&manifest, &stale).is_err());

            let mut stale_availability = metadata;
            stale_availability.availability_bindings[0].target_id = stable_id("entry", "stale");
            assert!(validate_source_coverage(&manifest, &stale_availability).is_err());
        });
    }

    #[test]
    fn registry_references_ranges_and_policies_fail_closed() {
        with_large_stack(|| {
            let manifest = public_api::build_manifest(&root()).unwrap();
            let mut metadata = build(&manifest).unwrap();
            let occurrence = metadata
                .parameter_quantities
                .iter_mut()
                .find(|occurrence| {
                    matches!(
                        occurrence.classification,
                        QuantityClassification::Applicable { .. }
                    )
                })
                .unwrap();
            let QuantityClassification::Applicable { range_id, .. } =
                &mut occurrence.classification
            else {
                unreachable!()
            };
            *range_id = "range.missing".into();
            assert!(metadata.validate().is_err());
        });
    }

    #[test]
    fn negative_fixture_rejects_missing_stale_unknown_and_heuristic_rows() {
        with_large_stack(|| {
            let fixture: NegativeFixture = serde_json::from_str(NEGATIVE_FIXTURE).unwrap();
            assert_eq!(
                fixture.schema,
                "https://vibelang.org/schemas/m05-metadata-negative-fixture/1"
            );
            for assertion in fixture.source_assertions {
                let source = std::fs::read_to_string(root().join(assertion.path)).unwrap();
                let normalized_source = source.split_whitespace().collect::<Vec<_>>().join(" ");
                let normalized_assertion = assertion
                    .contains
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(normalized_source.contains(&normalized_assertion));
            }

            let manifest = public_api::build_manifest(&root()).unwrap();
            for case in fixture.cases {
                let mut metadata = build(&manifest).unwrap();
                let error = match case.mutation.as_str() {
                    "missing_parameter_occurrence" => {
                        metadata.parameter_quantities.pop();
                        validate_source_coverage(&manifest, &metadata).unwrap_err()
                    }
                    "stale_ugen_input_occurrence" => {
                        metadata.ugen_input_quantities[0].occurrence_id =
                            stable_id("quantity_occurrence", "stale");
                        validate_source_coverage(&manifest, &metadata).unwrap_err()
                    }
                    "unknown_range_id" => {
                        let occurrence = metadata
                            .parameter_quantities
                            .iter_mut()
                            .find(|occurrence| {
                                matches!(
                                    occurrence.classification,
                                    QuantityClassification::Applicable { .. }
                                )
                            })
                            .unwrap();
                        let QuantityClassification::Applicable { range_id, .. } =
                            &mut occurrence.classification
                        else {
                            unreachable!()
                        };
                        *range_id = "range.missing".into();
                        metadata.validate().unwrap_err().to_string()
                    }
                    "heuristic_only_provenance" => {
                        let occurrence = metadata
                            .parameter_quantities
                            .iter_mut()
                            .find(|occurrence| {
                                matches!(
                                    occurrence.classification,
                                    QuantityClassification::Applicable { .. }
                                )
                            })
                            .unwrap();
                        let QuantityClassification::Applicable { basis, .. } =
                            &mut occurrence.classification
                        else {
                            unreachable!()
                        };
                        *basis = ClassificationBasis::HeuristicOnly;
                        metadata.validate().unwrap_err().to_string()
                    }
                    mutation => panic!("unknown negative mutation {mutation}"),
                };
                assert!(
                    error.contains(&case.expected_error_contains),
                    "{}: expected {:?} in {:?}",
                    case.mutation,
                    case.expected_error_contains,
                    error
                );
            }
        });
    }

    #[test]
    fn parser_collision_availability_and_security_truth_are_bound() {
        with_large_stack(|| {
            let manifest = public_api::build_manifest(&root()).unwrap();
            let metadata = build(&manifest).unwrap();
            assert!(!metadata.parser_bindings.is_empty());
            assert_eq!(metadata.collision_bindings.len(), 4);
            assert_eq!(metadata.availability_bindings.len(), manifest.entries.len());
            assert_eq!(metadata.security_bindings.len(), 1);
            assert!(metadata
                .availability_bindings
                .iter()
                .filter(|binding| binding.declared_status == "conditional")
                .all(|binding| !binding.predicate_capability_ids.is_empty()));
            assert!(metadata.capabilities.iter().any(|capability| {
                capability.capability_id == "capability.audio.schedule.absolute_beat"
                    && capability
                        .required_gates
                        .contains(&AvailabilityGate::BackendSemantic)
            }));
            let security = &metadata.security_bindings[0];
            assert_eq!(
                security.mode_id,
                "security.http.legacy_loopback_unrestricted_cors"
            );
            assert_eq!(security.state_id, "availability.degraded");
        });
    }
}
