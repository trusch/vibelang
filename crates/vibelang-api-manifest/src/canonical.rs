use crate::{ErrorCode, ManifestError};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Number, Value};
use std::{cmp::Ordering, fmt, str::FromStr};

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn canonical_json(value: &Value) -> Result<Vec<u8>, ManifestError> {
    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

pub fn canonical_json_of<T: Serialize>(value: &T) -> Result<Vec<u8>, ManifestError> {
    let value = serde_json::to_value(value)
        .map_err(|error| ManifestError::new(ErrorCode::JsonDecode, "$", error.to_string()))?;
    canonical_json(&value)
}

pub fn canonical_sha256<T: Serialize>(value: &T) -> Result<[u8; 32], ManifestError> {
    Ok(sha256(&canonical_json_of(value)?))
}

pub fn canonical_sha256_hex<T: Serialize>(value: &T) -> Result<String, ManifestError> {
    Ok(hex(&canonical_sha256(value)?))
}

pub fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes(chunk[offset..offset + 4].try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h].into_iter()) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.into_iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

pub fn sha256_hex(input: &[u8]) -> String {
    hex(&sha256(input))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), ManifestError> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(value) => output.extend_from_slice(canonical_number(value)?.as_bytes()),
        Value::String(value) => write_string(value, output),
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(values) => {
            let mut fields: Vec<_> = values.iter().collect();
            fields.sort_by(|(left, _), (right, _)| utf16_cmp(left, right));
            output.push(b'{');
            for (index, (key, value)) in fields.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_string(key, output);
                output.push(b':');
                write_value(value, output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn utf16_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn write_string(value: &str, output: &mut Vec<u8>) {
    output.push(b'"');
    for character in value.chars() {
        match character {
            '"' => output.extend_from_slice(br#"\""#),
            '\\' => output.extend_from_slice(br#"\\"#),
            '\u{0008}' => output.extend_from_slice(br#"\b"#),
            '\u{0009}' => output.extend_from_slice(br#"\t"#),
            '\u{000a}' => output.extend_from_slice(br#"\n"#),
            '\u{000c}' => output.extend_from_slice(br#"\f"#),
            '\u{000d}' => output.extend_from_slice(br#"\r"#),
            character if character <= '\u{001f}' => {
                let value = character as u32;
                output.extend_from_slice(br#"\u00"#);
                const HEX: &[u8; 16] = b"0123456789abcdef";
                output.push(HEX[((value >> 4) & 0x0f) as usize]);
                output.push(HEX[(value & 0x0f) as usize]);
            }
            character => {
                let mut buffer = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    output.push(b'"');
}

fn canonical_number(number: &Number) -> Result<String, ManifestError> {
    if let Some(value) = number.as_i64() {
        if value.unsigned_abs() > MAX_SAFE_INTEGER {
            return Err(unsafe_integer(number));
        }
        return Ok(value.to_string());
    }
    if let Some(value) = number.as_u64() {
        if value > MAX_SAFE_INTEGER {
            return Err(unsafe_integer(number));
        }
        return Ok(value.to_string());
    }
    let value = number.as_f64().ok_or_else(|| {
        ManifestError::new(
            ErrorCode::InvalidCanonicalJson,
            "$",
            format!("number {number} is not representable as IEEE-754 binary64"),
        )
    })?;
    if !value.is_finite() {
        return Err(ManifestError::new(
            ErrorCode::InvalidCanonicalJson,
            "$",
            "RFC 8785 forbids non-finite numbers",
        ));
    }
    if value == 0.0 {
        return Ok("0".into());
    }

    let raw = serde_json::to_string(&value).map_err(|error| {
        ManifestError::new(ErrorCode::InvalidCanonicalJson, "$", error.to_string())
    })?;
    Ok(ecmascript_number(&raw))
}

fn unsafe_integer(number: &Number) -> ManifestError {
    ManifestError::new(
        ErrorCode::InvalidCanonicalJson,
        "$",
        format!("integer {number} exceeds the exact IEEE-754 range; encode counters as strings"),
    )
}

fn ecmascript_number(raw: &str) -> String {
    let (negative, raw) = raw
        .strip_prefix('-')
        .map_or((false, raw), |value| (true, value));
    let (mantissa, exponent) = raw
        .split_once(['e', 'E'])
        .map_or((raw, 0), |(mantissa, exponent)| {
            (mantissa, exponent.parse::<i32>().unwrap())
        });
    let integer_digits = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let mut digits: String = mantissa
        .chars()
        .filter(|character| *character != '.')
        .collect();
    let leading = digits.bytes().take_while(|byte| *byte == b'0').count();
    let decimal_position = integer_digits + exponent - leading as i32;
    digits.drain(..leading);
    while digits.ends_with('0') {
        digits.pop();
    }
    if digits.is_empty() {
        return "0".into();
    }

    let scientific_exponent = decimal_position - 1;
    let mut output = String::new();
    if negative {
        output.push('-');
    }
    if (-6..21).contains(&scientific_exponent) {
        if decimal_position <= 0 {
            output.push_str("0.");
            output.extend(std::iter::repeat_n('0', (-decimal_position) as usize));
            output.push_str(&digits);
        } else if decimal_position as usize >= digits.len() {
            output.push_str(&digits);
            output.extend(std::iter::repeat_n(
                '0',
                decimal_position as usize - digits.len(),
            ));
        } else {
            let fractional = digits.split_off(decimal_position as usize);
            output.push_str(&digits);
            output.push('.');
            output.push_str(&fractional);
        }
    } else {
        output.push(digits.remove(0));
        if !digits.is_empty() {
            output.push('.');
            output.push_str(&digits);
        }
        output.push('e');
        if scientific_exponent >= 0 {
            output.push('+');
        }
        output.push_str(&scientific_exponent.to_string());
    }
    output
}

#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct DecimalCounter(u64);

impl DecimalCounter {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DecimalCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for DecimalCounter {
    type Err = ManifestError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ManifestError::new(
                ErrorCode::InvalidCounter,
                "$",
                format!("counter must be a canonical unsigned decimal string, got {value:?}"),
            ));
        }
        value
            .parse::<u64>()
            .map(Self)
            .map_err(|error| ManifestError::new(ErrorCode::InvalidCounter, "$", error.to_string()))
    }
}

impl Serialize for DecimalCounter {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for DecimalCounter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha256_matches_fips_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn decimal_counter_rejects_noncanonical_strings() {
        for invalid in ["", "00", "+1", "-1", "1.0", "18446744073709551616"] {
            assert!(invalid.parse::<DecimalCounter>().is_err(), "{invalid}");
        }
        assert_eq!(
            "18446744073709551615"
                .parse::<DecimalCounter>()
                .unwrap()
                .get(),
            u64::MAX
        );
    }

    #[test]
    fn canonical_key_order_uses_utf16_code_units() {
        let value = json!({"\u{10000}": 1, "\u{e000}": 2});
        assert_eq!(
            String::from_utf8(canonical_json(&value).unwrap()).unwrap(),
            "{\"𐀀\":1,\"\":2}"
        );
    }
}
