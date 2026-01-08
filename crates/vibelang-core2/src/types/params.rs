//! Parameter types for synth control.
//!
//! This module provides types for passing parameters to synths:
//!
//! - [`ParamMap`]: A collection of named parameter values
//! - [`ParamBuilder`]: Fluent builder for creating parameter maps
//!
//! # Common Parameter Names
//!
//! | Parameter | Description | Typical Range |
//! |-----------|-------------|---------------|
//! | `freq`    | Frequency in Hz | 20.0 - 20000.0 |
//! | `amp`     | Amplitude/volume | 0.0 - 1.0 |
//! | `pan`     | Stereo position | -1.0 (L) to 1.0 (R) |
//! | `gate`    | Note gate | 0.0 (off) or 1.0 (on) |
//! | `out`     | Output bus | 0+ |
//! | `buf`     | Buffer ID | 0+ |
//! | `rate`    | Playback rate | 0.0 - 4.0 |
//!
//! # Example
//!
//! ```
//! use vibelang_core2::types::params::ParamBuilder;
//!
//! // Using the builder pattern
//! let params = ParamBuilder::new()
//!     .param("freq", 440.0)
//!     .param("amp", 0.5)
//!     .param("pan", 0.0)
//!     .build();
//!
//! assert_eq!(params.get("freq"), Some(&440.0));
//! ```

use std::collections::HashMap;

/// A map of parameter names to values.
///
/// Used for passing control values to synths, patterns, effects, etc.
/// All synth parameters in SuperCollider are f32 values.
///
/// # Example
///
/// ```
/// use vibelang_core2::types::params::{ParamMap, ParamMapExt};
///
/// let mut params = ParamMap::new();
/// ParamMapExt::insert(&mut params, "freq", 440.0);
/// ParamMapExt::insert(&mut params, "amp", 0.5);
///
/// assert_eq!(params.get("freq"), Some(&440.0));
/// ```
pub type ParamMap = HashMap<String, f32>;

/// Extension trait for ParamMap convenience methods.
pub trait ParamMapExt {
    /// Insert a parameter with a string key.
    fn insert(&mut self, key: impl Into<String>, value: f32);

    /// Create a new empty parameter map.
    fn new() -> Self;

    /// Create a parameter map with a single value.
    fn with(key: impl Into<String>, value: f32) -> Self;
}

impl ParamMapExt for ParamMap {
    fn insert(&mut self, key: impl Into<String>, value: f32) {
        HashMap::insert(self, key.into(), value);
    }

    fn new() -> Self {
        HashMap::new()
    }

    fn with(key: impl Into<String>, value: f32) -> Self {
        let mut map = HashMap::new();
        map.insert(key.into(), value);
        map
    }
}

/// Builder for creating parameter maps fluently.
///
/// # Example
///
/// ```
/// use vibelang_core2::types::params::ParamBuilder;
///
/// let params = ParamBuilder::new()
///     .param("freq", 440.0)
///     .param("amp", 0.5)
///     .param("pan", 0.0)
///     .build();
///
/// assert_eq!(params.len(), 3);
/// ```
#[derive(Clone, Debug, Default)]
pub struct ParamBuilder {
    params: ParamMap,
}

impl ParamBuilder {
    /// Create a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a parameter.
    pub fn param(mut self, key: impl Into<String>, value: f32) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    /// Build the final parameter map.
    pub fn build(self) -> ParamMap {
        self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_map() {
        let mut params = ParamMap::new();
        ParamMapExt::insert(&mut params, "freq", 440.0);
        ParamMapExt::insert(&mut params, "amp", 0.8);

        assert_eq!(params.get("freq"), Some(&440.0));
        assert_eq!(params.get("amp"), Some(&0.8));
        assert_eq!(params.get("missing"), None);
    }

    #[test]
    fn test_param_builder() {
        let params = ParamBuilder::new()
            .param("freq", 880.0)
            .param("amp", 0.5)
            .build();

        assert_eq!(params.len(), 2);
        assert_eq!(params.get("freq"), Some(&880.0));
    }

    #[test]
    fn test_param_map_with() {
        let params: ParamMap = ParamMapExt::with("freq", 440.0);
        assert_eq!(params.get("freq"), Some(&440.0));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_param_builder_empty() {
        let params = ParamBuilder::new().build();
        assert!(params.is_empty());
    }

    #[test]
    fn test_param_builder_chaining() {
        let params = ParamBuilder::new()
            .param("freq", 440.0)
            .param("amp", 0.5)
            .param("pan", -0.5)
            .param("gate", 1.0)
            .param("out", 0.0)
            .build();

        assert_eq!(params.len(), 5);
        assert_eq!(params.get("freq"), Some(&440.0));
        assert_eq!(params.get("amp"), Some(&0.5));
        assert_eq!(params.get("pan"), Some(&-0.5));
        assert_eq!(params.get("gate"), Some(&1.0));
        assert_eq!(params.get("out"), Some(&0.0));
    }

    #[test]
    fn test_param_builder_override() {
        // Later params should override earlier ones
        let params = ParamBuilder::new()
            .param("freq", 440.0)
            .param("freq", 880.0)
            .build();

        assert_eq!(params.get("freq"), Some(&880.0));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_param_builder_clone() {
        let builder = ParamBuilder::new().param("freq", 440.0);
        let cloned = builder.clone();

        let params1 = builder.param("amp", 0.5).build();
        let params2 = cloned.param("pan", 0.0).build();

        assert_eq!(params1.len(), 2);
        assert_eq!(params2.len(), 2);
        assert!(params1.contains_key("amp"));
        assert!(params2.contains_key("pan"));
    }

    #[test]
    fn test_param_string_key() {
        let params = ParamBuilder::new()
            .param(String::from("freq"), 440.0)
            .param("amp", 0.5)  // &str also works
            .build();

        assert_eq!(params.len(), 2);
    }
}
