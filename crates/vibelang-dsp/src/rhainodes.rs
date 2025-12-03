//! NodeRef type for Rhai - opaque handles to graph nodes that support arithmetic.
//!
//! NodeRef is the primary type used in synthdef bodies to represent signals.
//! It supports arithmetic operations (+, -, *, /) that create new UGen nodes.

use super::errors::*;
use super::graph::*;
use std::fmt;

/// An opaque reference to a node in the UGen graph.
///
/// NodeRefs support arithmetic operations that create new nodes.
/// The value encodes both the node ID and output index:
/// - Lower 16 bits: node ID
/// - Upper 16 bits: output index (for multi-output UGens)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeRef(pub u32);

impl NodeRef {
    /// Create a NodeRef with the given ID (output index 0).
    pub fn new(id: u32) -> Self {
        NodeRef(id)
    }

    /// Create a NodeRef with a specific output channel.
    pub fn new_with_output(node_id: u32, output_index: u32) -> Self {
        NodeRef((output_index << 16) | (node_id & 0xFFFF))
    }

    /// Get the node ID.
    pub fn id(&self) -> u32 {
        self.0 & 0xFFFF
    }

    /// Get the output index.
    pub fn output_index(&self) -> u32 {
        self.0 >> 16
    }

    /// Convert to an Input for use in UGen graphs.
    ///
    /// Handles special encoding for parameter references.
    pub fn to_input(self) -> Input {
        // Check if this is a parameter reference (encoded as 0xFFFFFFFF - param_index)
        let node_id = self.id();
        let output_idx = self.output_index();

        if self.0 >= 0x80000000 {
            let param_index = 0xFFFFFFFF - self.0;
            // Parameters reference Control UGen (node 0) with output_index = param_index
            Input::Node {
                node_id: 0,
                output_index: param_index,
            }
        } else {
            Input::Node {
                node_id,
                output_index: output_idx,
            }
        }
    }

    /// Add two NodeRefs (creates a BinaryOpUGen with Add).
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 0) // 0 = Add
        })
    }

    /// Add a NodeRef and a float.
    pub fn add_float(self, other: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(other as f32);
            let inputs = vec![self.to_input(), Input::Constant(other as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 0)
        })
    }

    /// Subtract two NodeRefs.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 1) // 1 = Sub
        })
    }

    /// Subtract a float from a NodeRef.
    pub fn sub_float(self, other: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(other as f32);
            let inputs = vec![self.to_input(), Input::Constant(other as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 1)
        })
    }

    /// Multiply two NodeRefs.
    #[allow(clippy::should_implement_trait)]
    pub fn mul(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 2) // 2 = Mul
        })
    }

    /// Multiply a NodeRef by a float.
    pub fn mul_float(self, other: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(other as f32);
            let inputs = vec![self.to_input(), Input::Constant(other as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 2)
        })
    }

    /// Divide two NodeRefs.
    #[allow(clippy::should_implement_trait)]
    pub fn div(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 4) // 4 = Div
        })
    }

    /// Divide a NodeRef by a float.
    pub fn div_float(self, other: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(other as f32);
            let inputs = vec![self.to_input(), Input::Constant(other as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 4)
        })
    }

    /// Calculate hyperbolic tangent.
    pub fn tanh(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 28) // 28 = tanh
        })
    }

    /// Clip signal to range [lo, hi].
    pub fn clip(self, lo: f64, hi: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(lo as f32);
            builder.add_constant(hi as f32);
            let inputs = vec![
                self.to_input(),
                Input::Constant(lo as f32),
                Input::Constant(hi as f32),
            ];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("Clip".to_string(), rate, inputs, 1, 0)
        })
    }

    /// Absolute value.
    pub fn abs(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 5) // 5 = abs
        })
    }

    /// Sign (-1, 0, or 1).
    pub fn sign(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 11) // 11 = sign
        })
    }

    /// Square (x * x).
    pub fn squared(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 12) // 12 = squared
        })
    }

    /// Cube (x * x * x).
    pub fn cubed(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 13) // 13 = cubed
        })
    }

    /// Square root.
    pub fn sqrt(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 14) // 14 = sqrt
        })
    }

    /// Exponential.
    pub fn exp(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 15) // 15 = exp
        })
    }

    /// Natural logarithm.
    pub fn ln(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 25) // 25 = log
        })
    }

    /// Distortion via soft clipping (distort algorithm).
    pub fn distort(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 42) // 42 = distort
        })
    }

    /// Soft clip (attempt to keep signal in -1 to 1).
    pub fn softclip(self) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("UnaryOpUGen".to_string(), rate, inputs, 1, 43) // 43 = softclip
        })
    }

    /// Minimum of two signals.
    pub fn min(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 12) // 12 = min
        })
    }

    /// Maximum of two signals.
    pub fn max(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 13) // 13 = max
        })
    }

    /// Power (x^y).
    pub fn pow(self, exponent: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(exponent as f32);
            let inputs = vec![self.to_input(), Input::Constant(exponent as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 25) // 25 = pow
        })
    }

    /// Power with NodeRef exponent.
    pub fn pow_node(self, other: NodeRef) -> Result<NodeRef> {
        with_builder(|builder| {
            let inputs = vec![self.to_input(), other.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 25) // 25 = pow
        })
    }

    /// Modulo.
    pub fn modulo(self, divisor: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(divisor as f32);
            let inputs = vec![self.to_input(), Input::Constant(divisor as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 5) // 5 = mod
        })
    }

    /// Round to nearest multiple.
    pub fn round_to(self, multiple: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(multiple as f32);
            let inputs = vec![self.to_input(), Input::Constant(multiple as f32)];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 19) // 19 = round
        })
    }

    /// Wrap signal to range (like modulo but for signed signals).
    pub fn wrap(self, lo: f64, hi: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(lo as f32);
            builder.add_constant(hi as f32);
            let inputs = vec![
                self.to_input(),
                Input::Constant(lo as f32),
                Input::Constant(hi as f32),
            ];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("Wrap".to_string(), rate, inputs, 1, 0)
        })
    }

    /// Fold signal back into range.
    pub fn fold(self, lo: f64, hi: f64) -> Result<NodeRef> {
        with_builder(|builder| {
            builder.add_constant(lo as f32);
            builder.add_constant(hi as f32);
            let inputs = vec![
                self.to_input(),
                Input::Constant(lo as f32),
                Input::Constant(hi as f32),
            ];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("Fold".to_string(), rate, inputs, 1, 0)
        })
    }

    /// Linear interpolation to another signal.
    pub fn lerp(self, other: NodeRef, amount: NodeRef) -> Result<NodeRef> {
        // lerp(a, b, t) = a + (b - a) * t
        let diff = other.sub(self)?;
        let scaled = diff.mul(amount)?;
        self.add(scaled)
    }
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeRef({})", self.0)
    }
}

/// Register NodeRef type and operators with a Rhai engine.
pub fn register_node_ref(engine: &mut rhai::Engine) {
    engine.register_type::<NodeRef>();

    // NodeRef + NodeRef
    engine.register_fn("+", |a: NodeRef, b: NodeRef| a.add(b).unwrap());
    // NodeRef + FLOAT
    engine.register_fn("+", |a: NodeRef, b: f64| a.add_float(b).unwrap());
    // FLOAT + NodeRef
    engine.register_fn("+", |a: f64, b: NodeRef| b.add_float(a).unwrap());

    // NodeRef - NodeRef
    engine.register_fn("-", |a: NodeRef, b: NodeRef| a.sub(b).unwrap());
    // NodeRef - FLOAT
    engine.register_fn("-", |a: NodeRef, b: f64| a.sub_float(b).unwrap());
    // FLOAT - NodeRef (reverse)
    engine.register_fn("-", |a: f64, b: NodeRef| {
        with_builder(|builder| {
            builder.add_constant(a as f32);
            let inputs = vec![Input::Constant(a as f32), b.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 1)
        })
        .unwrap()
    });

    // NodeRef * NodeRef
    engine.register_fn("*", |a: NodeRef, b: NodeRef| a.mul(b).unwrap());
    // NodeRef * FLOAT
    engine.register_fn("*", |a: NodeRef, b: f64| a.mul_float(b).unwrap());
    // FLOAT * NodeRef
    engine.register_fn("*", |a: f64, b: NodeRef| b.mul_float(a).unwrap());

    // NodeRef / NodeRef
    engine.register_fn("/", |a: NodeRef, b: NodeRef| a.div(b).unwrap());
    // NodeRef / FLOAT
    engine.register_fn("/", |a: NodeRef, b: f64| a.div_float(b).unwrap());
    // FLOAT / NodeRef (reverse)
    engine.register_fn("/", |a: f64, b: NodeRef| {
        with_builder(|builder| {
            builder.add_constant(a as f32);
            let inputs = vec![Input::Constant(a as f32), b.to_input()];
            let rate = builder.max_rate_from_inputs(&inputs);
            builder.add_node("BinaryOpUGen".to_string(), rate, inputs, 1, 4)
        })
        .unwrap()
    });

    // Unary Ops
    engine.register_fn("tanh", |a: NodeRef| a.tanh().unwrap());
    engine.register_fn("abs", |a: NodeRef| a.abs().unwrap());
    engine.register_fn("sign", |a: NodeRef| a.sign().unwrap());
    engine.register_fn("squared", |a: NodeRef| a.squared().unwrap());
    engine.register_fn("cubed", |a: NodeRef| a.cubed().unwrap());
    engine.register_fn("sqrt", |a: NodeRef| a.sqrt().unwrap());
    engine.register_fn("exp", |a: NodeRef| a.exp().unwrap());
    engine.register_fn("ln", |a: NodeRef| a.ln().unwrap());
    engine.register_fn("distort", |a: NodeRef| a.distort().unwrap());
    engine.register_fn("softclip", |a: NodeRef| a.softclip().unwrap());

    // Signal processing
    engine.register_fn("clip", |a: NodeRef, lo: f64, hi: f64| a.clip(lo, hi).unwrap());
    engine.register_fn("wrap", |a: NodeRef, lo: f64, hi: f64| a.wrap(lo, hi).unwrap());
    engine.register_fn("fold", |a: NodeRef, lo: f64, hi: f64| a.fold(lo, hi).unwrap());

    // Binary math ops
    engine.register_fn("min", |a: NodeRef, b: NodeRef| a.min(b).unwrap());
    engine.register_fn("max", |a: NodeRef, b: NodeRef| a.max(b).unwrap());
    engine.register_fn("pow", |a: NodeRef, b: f64| a.pow(b).unwrap());
    engine.register_fn("modulo", |a: NodeRef, b: f64| a.modulo(b).unwrap());
    engine.register_fn("round_to", |a: NodeRef, b: f64| a.round_to(b).unwrap());

    // Interpolation
    engine.register_fn("lerp", |a: NodeRef, b: NodeRef, t: NodeRef| a.lerp(b, t).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_ref_id_extraction() {
        let node = NodeRef::new(42);
        assert_eq!(node.id(), 42);
        assert_eq!(node.output_index(), 0);
    }

    #[test]
    fn test_node_ref_with_output() {
        let node = NodeRef::new_with_output(10, 3);
        assert_eq!(node.id(), 10);
        assert_eq!(node.output_index(), 3);
    }

    #[test]
    fn test_node_ref_display() {
        let node = NodeRef::new(5);
        assert_eq!(format!("{}", node), "NodeRef(5)");
    }
}
