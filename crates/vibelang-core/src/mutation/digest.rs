use super::PublicDigest;
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::fmt;
use subtle::{Choice, ConstantTimeEq};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, thiserror::Error)]
pub enum DigestError {
    #[error("canonical request serialization failed: {0}")]
    Canonical(String),
    #[error("epoch fingerprint key generation failed: {0}")]
    Random(String),
    #[error("request redaction failed: {0}")]
    Redaction(String),
}

pub trait RedactionHook {
    fn redact(&self, semantic_request: &Value) -> Result<Option<Value>, DigestError>;
}

impl<F> RedactionHook for F
where
    F: Fn(&Value) -> Result<Option<Value>, DigestError>,
{
    fn redact(&self, semantic_request: &Value) -> Result<Option<Value>, DigestError> {
        self(semantic_request)
    }
}

#[derive(Clone)]
pub struct RequestMaterial {
    semantic: Value,
    public_redacted: Option<Value>,
}

impl fmt::Debug for RequestMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestMaterial")
            .field("semantic", &"<redacted>")
            .field("public_redacted", &self.public_redacted)
            .finish()
    }
}

impl RequestMaterial {
    pub fn new<T, R>(semantic: &T, public_redacted: Option<&R>) -> Result<Self, DigestError>
    where
        T: Serialize,
        R: Serialize,
    {
        let semantic = serde_json::to_value(semantic)
            .map_err(|error| DigestError::Canonical(error.to_string()))?;
        let public_redacted = public_redacted
            .map(serde_json::to_value)
            .transpose()
            .map_err(|error| DigestError::Canonical(error.to_string()))?;
        Ok(Self {
            semantic,
            public_redacted,
        })
    }

    pub fn with_redactor<T>(
        semantic: &T,
        redactor: &impl RedactionHook,
    ) -> Result<Self, DigestError>
    where
        T: Serialize,
    {
        let semantic = serde_json::to_value(semantic)
            .map_err(|error| DigestError::Canonical(error.to_string()))?;
        let public_redacted = redactor.redact(&semantic)?;
        Ok(Self {
            semantic,
            public_redacted,
        })
    }

    #[must_use]
    pub fn from_values(semantic: Value, public_redacted: Option<Value>) -> Self {
        Self {
            semantic,
            public_redacted,
        }
    }

    pub(crate) fn request_fingerprint<T: Serialize>(
        &self,
        key: &EpochFingerprintKey,
        policy: &T,
    ) -> Result<RequestFingerprint, DigestError> {
        let canonical = canonical(&(policy, &self.semantic))?;
        Ok(RequestFingerprint(hmac(
            &key.0,
            b"vibelang/request-fingerprint/v1\0",
            &canonical,
        )))
    }

    pub(crate) fn public_digest(&self) -> Result<Option<PublicDigest>, DigestError> {
        self.public_redacted.as_ref().map(public_digest).transpose()
    }
}

#[derive(Clone)]
pub(crate) struct EpochFingerprintKey([u8; 32]);

impl EpochFingerprintKey {
    pub(crate) fn generate() -> Result<Self, DigestError> {
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|error| DigestError::Random(error.to_string()))?;
        Ok(Self(key))
    }

    #[cfg(test)]
    pub(crate) const fn for_test(key: [u8; 32]) -> Self {
        Self(key)
    }

    pub(crate) fn key_fingerprint(
        &self,
        caller_namespace: &str,
        idempotency_key: &str,
    ) -> Result<IdempotencyKeyFingerprint, DigestError> {
        let canonical = canonical(&(caller_namespace, idempotency_key))?;
        Ok(IdempotencyKeyFingerprint(hmac(
            &self.0,
            b"vibelang/idempotency-key/v1\0",
            &canonical,
        )))
    }
}

impl fmt::Debug for EpochFingerprintKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EpochFingerprintKey(<redacted>)")
    }
}

impl Drop for EpochFingerprintKey {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

pub(crate) struct RetainedIdentityKey([u8; 32]);

impl RetainedIdentityKey {
    pub(crate) fn generate() -> Result<Self, DigestError> {
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|error| DigestError::Random(error.to_string()))?;
        Ok(Self(key))
    }

    pub(crate) fn key_fingerprint(
        &self,
        caller_namespace: &str,
        idempotency_key: &str,
    ) -> Result<RetainedIdentityFingerprint, DigestError> {
        let canonical = canonical(&(caller_namespace, idempotency_key))?;
        Ok(RetainedIdentityFingerprint(hmac(
            &self.0,
            b"vibelang/idempotency-key/reset-retention/v1\0",
            &canonical,
        )))
    }
}

impl fmt::Debug for RetainedIdentityKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RetainedIdentityKey(<redacted>)")
    }
}

impl Drop for RetainedIdentityKey {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

pub(crate) struct RetainedIdentityFingerprint([u8; 32]);

impl RetainedIdentityFingerprint {
    pub(crate) fn constant_time_eq(&self, other: &Self) -> Choice {
        self.0.ct_eq(&other.0)
    }
}

impl fmt::Debug for RetainedIdentityFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RetainedIdentityFingerprint(<redacted>)")
    }
}

impl Drop for RetainedIdentityFingerprint {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone)]
pub(crate) struct RequestFingerprint([u8; 32]);

impl RequestFingerprint {
    pub(crate) fn constant_time_eq(&self, other: &Self) -> bool {
        bool::from(self.0.ct_eq(&other.0))
    }
}

impl fmt::Debug for RequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestFingerprint(<redacted>)")
    }
}

impl Drop for RequestFingerprint {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct IdempotencyKeyFingerprint([u8; 32]);

impl fmt::Debug for IdempotencyKeyFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdempotencyKeyFingerprint(<redacted>)")
    }
}

impl Drop for IdempotencyKeyFingerprint {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

pub(crate) fn operation_digest<T: Serialize>(value: &T) -> Result<PublicDigest, DigestError> {
    public_digest(value)
}

fn public_digest<T: Serialize>(value: &T) -> Result<PublicDigest, DigestError> {
    let canonical = canonical(value)?;
    let digest = Sha256::digest(canonical);
    Ok(PublicDigest::sha256(lower_hex(&digest)))
}

fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, DigestError> {
    serde_jcs::to_vec(value).map_err(|error| DigestError::Canonical(error.to_string()))
}

fn hmac(key: &[u8], domain: &[u8], value: &[u8]) -> [u8; 32] {
    let mut mac = match HmacSha256::new_from_slice(key) {
        Ok(mac) => mac,
        Err(_) => unreachable!("HMAC-SHA-256 accepts every key length"),
    };
    mac.update(domain);
    mac.update(value);
    mac.finalize().into_bytes().into()
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
