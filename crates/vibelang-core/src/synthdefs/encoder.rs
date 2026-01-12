//! Minimal SynthDef encoder for built-in synthdefs.
//!
//! This is a simplified encoder that only handles what's needed for
//! the built-in synthdefs. For full synthdef generation, use vibelang-dsp.

/// Rate of a UGen.
///
/// Represents SuperCollider's UGen calculation rates:
/// - `Scalar`: Computed once at synth instantiation
/// - `Control`: Computed once per control period (64 samples by default)
/// - `Audio`: Computed for every sample
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Scalar not currently used but part of complete API
pub enum Rate {
    Scalar = 0,
    Control = 1,
    Audio = 2,
}

/// Input to a UGen.
#[derive(Clone, Debug)]
pub enum Input {
    /// A constant value (index into constant table).
    Constant { index: usize },
    /// A parameter value (index into parameter table).
    Param { index: usize },
    /// A reference to another node's output.
    Node { node_id: u32, output_index: u32 },
}

impl Input {
    /// Create a parameter input.
    pub fn param(index: usize) -> Self {
        Input::Param { index }
    }

    /// Create a node input.
    pub fn node(node_id: u32, output_index: u32) -> Self {
        Input::Node {
            node_id,
            output_index,
        }
    }
}

/// A UGen node.
struct UGenNode {
    name: String,
    rate: Rate,
    inputs: Vec<Input>,
    num_outputs: u32,
    special_index: i16,
}

/// Parameter specification.
struct ParamSpec {
    name: String,
    default: f32,
    index: usize,
}

/// Simple graph builder for synthdefs.
pub struct GraphBuilder {
    name: String,
    nodes: Vec<UGenNode>,
    constants: Vec<f32>,
    params: Vec<ParamSpec>,
}

impl GraphBuilder {
    /// Create a new graph builder.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            nodes: Vec::new(),
            constants: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Add a parameter.
    pub fn add_param(&mut self, name: &str, default: f32) -> usize {
        let index = self.params.len();
        self.params.push(ParamSpec {
            name: name.to_string(),
            default,
            index,
        });
        index
    }

    /// Add a constant, returns its index.
    pub fn add_constant(&mut self, value: f32) -> usize {
        // Check for existing constant
        for (i, &c) in self.constants.iter().enumerate() {
            if (c - value).abs() < 1e-9 {
                return i;
            }
        }
        let index = self.constants.len();
        self.constants.push(value);
        index
    }

    /// Create the Control UGen (must be called before adding other nodes).
    pub fn create_control_ugen(&mut self) {
        if self.params.is_empty() {
            return;
        }

        self.nodes.insert(
            0,
            UGenNode {
                name: "Control".to_string(),
                rate: Rate::Control,
                inputs: Vec::new(),
                num_outputs: self.params.len() as u32,
                special_index: 0,
            },
        );
    }

    /// Add a UGen node, returns its ID.
    pub fn add_node(
        &mut self,
        name: &str,
        rate: Rate,
        inputs: Vec<Input>,
        num_outputs: u32,
        special_index: i16,
    ) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(UGenNode {
            name: name.to_string(),
            rate,
            inputs,
            num_outputs,
            special_index,
        });
        id
    }

    /// Encode the synthdef to bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header: "SCgf"
        buf.extend_from_slice(b"SCgf");

        // Version: 2 (big-endian i32)
        buf.extend_from_slice(&2i32.to_be_bytes());

        // Number of definitions: 1 (big-endian i16)
        buf.extend_from_slice(&1i16.to_be_bytes());

        // SynthDef name (pstring)
        self.write_pstring(&mut buf, &self.name);

        // Constants (i32 count, then f32 values)
        buf.extend_from_slice(&(self.constants.len() as i32).to_be_bytes());
        for &c in &self.constants {
            buf.extend_from_slice(&c.to_be_bytes());
        }

        // Parameters (i32 count = number of slots)
        buf.extend_from_slice(&(self.params.len() as i32).to_be_bytes());
        for param in &self.params {
            buf.extend_from_slice(&param.default.to_be_bytes());
        }

        // Parameter names (i32 count, then pstring name + i32 index)
        buf.extend_from_slice(&(self.params.len() as i32).to_be_bytes());
        for param in &self.params {
            self.write_pstring(&mut buf, &param.name);
            buf.extend_from_slice(&(param.index as i32).to_be_bytes());
        }

        // UGens (i32 count, then UGen specs)
        buf.extend_from_slice(&(self.nodes.len() as i32).to_be_bytes());
        for node in &self.nodes {
            self.encode_ugen(&mut buf, node);
        }

        // Variants: 0 (i16)
        buf.extend_from_slice(&0i16.to_be_bytes());

        buf
    }

    fn encode_ugen(&self, buf: &mut Vec<u8>, node: &UGenNode) {
        // Name (pstring)
        self.write_pstring(buf, &node.name);

        // Rate (i8)
        buf.push(node.rate as u8);

        // Number of inputs (i32)
        buf.extend_from_slice(&(node.inputs.len() as i32).to_be_bytes());

        // Number of outputs (i32)
        buf.extend_from_slice(&(node.num_outputs as i32).to_be_bytes());

        // Special index (i16)
        buf.extend_from_slice(&node.special_index.to_be_bytes());

        // Inputs (each is i32 source, i32 output_index)
        for input in &node.inputs {
            match input {
                Input::Constant { index } => {
                    buf.extend_from_slice(&(-1i32).to_be_bytes()); // -1 = constant
                    buf.extend_from_slice(&(*index as i32).to_be_bytes());
                }
                Input::Param { index } => {
                    // Parameter references Control UGen (node 0) at the given output index
                    buf.extend_from_slice(&0i32.to_be_bytes()); // Control UGen is node 0
                    buf.extend_from_slice(&(*index as i32).to_be_bytes());
                }
                Input::Node {
                    node_id,
                    output_index,
                } => {
                    buf.extend_from_slice(&(*node_id as i32).to_be_bytes());
                    buf.extend_from_slice(&(*output_index as i32).to_be_bytes());
                }
            }
        }

        // Output rates (i8 array)
        for _ in 0..node.num_outputs {
            buf.push(node.rate as u8);
        }
    }

    fn write_pstring(&self, buf: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        buf.push(bytes.len() as u8);
        buf.extend_from_slice(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_synthdef() {
        let builder = GraphBuilder::new("test");
        let bytes = builder.encode();

        assert_eq!(&bytes[0..4], b"SCgf");
        assert_eq!(&bytes[4..8], &2i32.to_be_bytes());
    }

    #[test]
    fn test_pstring_encoding() {
        let builder = GraphBuilder::new("test");
        let mut buf = Vec::new();
        builder.write_pstring(&mut buf, "hello");
        assert_eq!(buf, vec![5, b'h', b'e', b'l', b'l', b'o']);
    }

    #[test]
    fn test_constant_dedup() {
        let mut builder = GraphBuilder::new("test");
        let idx1 = builder.add_constant(440.0);
        let idx2 = builder.add_constant(440.0);
        let idx3 = builder.add_constant(880.0);
        assert_eq!(idx1, idx2);
        assert_ne!(idx1, idx3);
    }
}
