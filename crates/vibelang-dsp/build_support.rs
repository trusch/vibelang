//! Registration-shape rules shared by DSP code generation and API extraction.

use serde::Deserialize;

pub const MAX_POSITIONAL_ARITY: usize = 20;
pub const MAX_DYNAMIC_PARAMETERS: usize = 16;
#[allow(dead_code)]
pub const DEMAND_QUARANTINE_REASON: &str =
    "demand-rate graph encoding and UGen-specific input lowering are not implemented";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum UGenNumericRange {
    FiniteUnbounded,
    FiniteNonnegative,
    FinitePositive,
    IntegerUnbounded,
    IntegerNonnegative,
}

impl UGenNumericRange {
    #[allow(dead_code)]
    pub const fn rust_variant(self) -> &'static str {
        match self {
            Self::FiniteUnbounded => "FiniteUnbounded",
            Self::FiniteNonnegative => "FiniteNonnegative",
            Self::FinitePositive => "FinitePositive",
            Self::IntegerUnbounded => "IntegerUnbounded",
            Self::IntegerNonnegative => "IntegerNonnegative",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct UGenManifest {
    pub name: String,
    pub description: String,
    pub rates: Vec<String>,
    pub inputs: Vec<UGenInput>,
    pub outputs: u32,
    pub category: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub functions: Option<Vec<String>>,
    /// Server-side UGen class to emit instead of `name`.
    #[serde(default)]
    pub ugen_class: Option<String>,
    /// Operator selector passed to the server. Defaults to zero.
    #[serde(default)]
    pub special_index: Option<i16>,
    /// Public shape argument removed from the encoded server inputs.
    #[serde(default)]
    #[allow(dead_code)]
    pub channel_count_input: Option<String>,
    /// Whether the name is an sclang helper, alias, or wrapper.
    #[serde(default)]
    pub pseudo: bool,
    /// SuperCollider plugin package required by this UGen.
    #[serde(default)]
    pub requires_plugin: Option<String>,
    /// Why a documentation-only entry cannot be generated.
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UGenInput {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    pub description: String,
}

pub fn positional_arity_max(input_count: usize) -> usize {
    input_count.min(MAX_POSITIONAL_ARITY)
}

pub fn has_array_overload(input_count: usize) -> bool {
    input_count > MAX_POSITIONAL_ARITY
}

pub fn ugen_numeric_range(registered_name: &str, input: &UGenInput) -> Option<UGenNumericRange> {
    if input.ty == "method" {
        return None;
    }

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
        Some(UGenNumericRange::FinitePositive)
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
    ) || matches!(name.as_str(), "amp" | "ampthreshold" | "gain")
    {
        Some(UGenNumericRange::FiniteNonnegative)
    } else if name == "bus" {
        Some(UGenNumericRange::IntegerNonnegative)
    } else if reviewed_ugen_count_name(&name)
        || matches!(
            (registered_name, name.as_str()),
            ("audio_msg_ar", "index") | ("dswitch1_demand", "index") | ("dswitch_demand", "index")
        )
        || input.ty == "int"
    {
        Some(UGenNumericRange::IntegerUnbounded)
    } else {
        Some(UGenNumericRange::FiniteUnbounded)
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

#[allow(dead_code)]
pub fn runtime_rate_rust(rate: &str) -> Option<&'static str> {
    match rate {
        "ar" => Some("Rate::Audio"),
        "kr" => Some("Rate::Control"),
        "ir" => Some("Rate::Scalar"),
        "demand" | "builder" => None,
        _ => None,
    }
}

#[allow(dead_code)]
pub fn runtime_rate_manifest(rate: &str) -> Option<&'static str> {
    match rate {
        "ar" => Some("audio"),
        "kr" => Some("control"),
        "ir" => Some("scalar"),
        "demand" | "builder" => None,
        _ => None,
    }
}

#[allow(dead_code)]
pub fn is_quarantined_rate(rate: &str) -> bool {
    rate == "demand"
}

pub fn to_snake_case(value: &str) -> String {
    if value == "DC" {
        return "dc".to_string();
    }
    let mut result = String::new();
    let chars: Vec<char> = value.chars().collect();
    for (index, &character) in chars.iter().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                let previous = chars[index - 1];
                let previous_lower = previous.is_lowercase();
                let next_lower = chars
                    .get(index + 1)
                    .map(|next| next.is_lowercase())
                    .unwrap_or(false);
                if (previous_lower || next_lower) && previous != '_' {
                    result.push('_');
                }
            }
            result.push(character.to_lowercase().next().unwrap());
        } else {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demand_has_no_runtime_rate_fallback() {
        assert_eq!(runtime_rate_rust("demand"), None);
        assert_eq!(runtime_rate_manifest("demand"), None);
        assert_eq!(runtime_rate_rust("unknown"), None);
        assert_eq!(runtime_rate_manifest("unknown"), None);
        assert!(is_quarantined_rate("demand"));
        assert!(!is_quarantined_rate("ar"));
    }

    #[test]
    fn arity_policy_matches_rhai_limit() {
        assert_eq!(positional_arity_max(24), 20);
        assert!(has_array_overload(24));
        assert!(!has_array_overload(20));
    }

    #[test]
    fn numeric_range_policy_matches_reviewed_ugen_categories() {
        let input = |name: &str, ty: &str| UGenInput {
            name: name.into(),
            ty: ty.into(),
            default: None,
            description: "fixture".into(),
        };

        assert_eq!(
            ugen_numeric_range("sin_osc_ar", &input("freq", "float")),
            Some(UGenNumericRange::FinitePositive)
        );
        assert_eq!(
            ugen_numeric_range("line_ar", &input("dur", "float")),
            Some(UGenNumericRange::FiniteNonnegative)
        );
        assert_eq!(
            ugen_numeric_range("out_ar", &input("bus", "float")),
            Some(UGenNumericRange::IntegerNonnegative)
        );
        assert_eq!(
            ugen_numeric_range("mfcc_kr", &input("numcoeff", "int")),
            Some(UGenNumericRange::IntegerUnbounded)
        );
        assert_eq!(
            ugen_numeric_range("sin_osc_ar", &input("phase", "float")),
            Some(UGenNumericRange::FiniteUnbounded)
        );
        assert_eq!(
            ugen_numeric_range("envelope", &input(".build()", "method")),
            None
        );
    }
}
