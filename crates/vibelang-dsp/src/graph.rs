//! Graph builder and IR for UGen graphs.
//!
//! This module provides the core data structures for building SuperCollider
//! synthesis graphs:
//!
//! - [`Rate`] - UGen calculation rate (audio, control, scalar)
//! - [`Input`] - Input to a UGen (constant or node output)
//! - [`UGenNode`] - A node in the synthesis graph
//! - [`ParamSpec`] - Parameter specification with defaults
//! - [`GraphBuilderInner`] - Mutable graph construction state
//! - [`GraphIR`] - Immutable graph ready for encoding

use super::errors::*;
use super::rhainodes::NodeRef;
use std::cell::RefCell;
use std::collections::HashMap;

/// Rate of a UGen (audio, control, scalar).
///
/// Ordering: Scalar < Control < Audio
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rate {
    /// Calculated once at synth creation.
    Scalar = 0,
    /// Calculated once per control block (~64 samples).
    Control = 1,
    /// Calculated every sample.
    Audio = 2,
}

impl Rate {
    /// Convert to the byte value used in scsyndef format.
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }
}

/// Input to a UGen - either a constant or another node's output.
#[derive(Clone, Debug)]
pub enum Input {
    /// A constant value.
    Constant(f32),
    /// A reference to another node's output.
    Node {
        /// Index of the source node in the graph.
        node_id: u32,
        /// Which output of that node (0 for single-output UGens).
        output_index: u32,
    },
}

/// A UGen node in the graph.
#[derive(Clone, Debug)]
pub struct UGenNode {
    /// UGen class name (e.g., "SinOsc", "EnvGen").
    pub name: String,
    /// Calculation rate.
    pub rate: Rate,
    /// Input connections.
    pub inputs: Vec<Input>,
    /// Number of output channels.
    pub num_outputs: u32,
    /// Special index (used for BinaryOpUGen, UnaryOpUGen operator codes).
    pub special_index: i16,
}

/// Parameter specification.
#[derive(Clone, Debug)]
pub struct ParamSpec {
    /// Parameter name.
    pub name: String,
    /// Default values (array for multi-channel parameters).
    pub default: Vec<f32>,
    /// Index into the flattened parameter array.
    pub index: usize,
    /// Optional lag time in milliseconds.
    pub lag_ms: Option<f32>,
}

/// The mutable state of a graph builder.
///
/// This is used during synthdef construction to accumulate nodes,
/// constants, and parameters.
pub struct GraphBuilderInner {
    /// UGen nodes in topological order.
    pub nodes: Vec<UGenNode>,
    /// Constant values used by the graph.
    pub constants: Vec<f32>,
    /// Parameter specifications.
    pub params: Vec<ParamSpec>,
    /// Map from parameter name to its index.
    pub param_map: HashMap<String, u32>,
    /// Output bus number.
    pub out_bus: i32,
    /// Optional tag for the output bus (for routing).
    pub out_bus_tag: Option<String>,
}

impl Default for GraphBuilderInner {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphBuilderInner {
    /// Create a new empty graph builder.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            constants: Vec::new(),
            params: Vec::new(),
            param_map: HashMap::new(),
            out_bus: 0,
            out_bus_tag: None,
        }
    }

    /// Add a constant to the graph, returns its index.
    ///
    /// Constants are deduplicated - if the value already exists,
    /// its existing index is returned.
    pub fn add_constant(&mut self, value: f32) -> usize {
        for (i, &c) in self.constants.iter().enumerate() {
            if (c - value).abs() < 1e-9 {
                return i;
            }
        }
        let idx = self.constants.len();
        self.constants.push(value);
        idx
    }

    /// Add a UGen node, returns its NodeRef.
    pub fn add_node(
        &mut self,
        name: String,
        rate: Rate,
        inputs: Vec<Input>,
        num_outputs: u32,
        special_index: i16,
    ) -> NodeRef {
        let id = self.nodes.len() as u32;
        self.nodes.push(UGenNode {
            name,
            rate,
            inputs,
            num_outputs,
            special_index,
        });
        NodeRef(id)
    }

    /// Add a parameter to the graph.
    ///
    /// For array parameters, pass a Vec with multiple values.
    /// For scalar parameters, pass a Vec with a single value.
    pub fn add_param(&mut self, name: String, default: Vec<f32>, lag_ms: Option<f32>) -> u32 {
        let id = self.params.len() as u32;
        let index = self.total_param_slots();
        self.params.push(ParamSpec {
            name: name.clone(),
            default,
            index,
            lag_ms,
        });
        self.param_map.insert(name, id);
        id
    }

    /// Compute the total number of parameter value slots.
    ///
    /// This accounts for array controls which consume multiple slots.
    pub fn total_param_slots(&self) -> usize {
        self.params.iter().map(|p| p.default.len()).sum()
    }

    /// Get a parameter ID by name.
    pub fn get_param(&self, name: &str) -> Option<u32> {
        self.param_map.get(name).copied()
    }

    /// Get the rate of a node by its ID.
    pub fn get_node_rate(&self, node_id: u32) -> Rate {
        self.nodes
            .get(node_id as usize)
            .map(|n| n.rate)
            .unwrap_or(Rate::Scalar)
    }

    /// Compute the maximum rate from a list of inputs.
    ///
    /// Used to determine the rate of BinaryOpUGen/UnaryOpUGen.
    pub fn max_rate_from_inputs(&self, inputs: &[Input]) -> Rate {
        let mut max_rate = Rate::Scalar;
        for input in inputs {
            let input_rate = match input {
                Input::Constant(_) => Rate::Scalar,
                Input::Node { node_id, .. } => self.get_node_rate(*node_id),
            };
            if input_rate > max_rate {
                max_rate = input_rate;
            }
        }
        max_rate
    }

    /// Create the Control UGen node for parameters.
    ///
    /// This should be called after all parameters are added, before any
    /// other nodes. Returns the total number of parameter slots.
    pub fn create_control_ugen(&mut self) -> u32 {
        if self.params.is_empty() {
            return 0;
        }

        let total_slots = self.total_param_slots() as u32;

        let control_node = UGenNode {
            name: "Control".to_string(),
            rate: Rate::Control,
            inputs: Vec::new(),
            num_outputs: total_slots,
            special_index: 0,
        };

        // Insert at the beginning
        self.nodes.insert(0, control_node);

        total_slots
    }
}

// Thread-local graph builder.
thread_local! {
    pub static GRAPH_BUILDER: RefCell<Option<GraphBuilderInner>> = const { RefCell::new(None) };
}

/// Set the active graph builder for the current thread.
pub fn set_active_builder(builder: GraphBuilderInner) {
    GRAPH_BUILDER.with(|gb| {
        *gb.borrow_mut() = Some(builder);
    });
}

/// Clear and return the active graph builder.
pub fn clear_active_builder() -> Option<GraphBuilderInner> {
    GRAPH_BUILDER.with(|gb| gb.borrow_mut().take())
}

/// Access the active graph builder with a closure.
///
/// Returns an error if no builder is active.
pub fn with_builder<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut GraphBuilderInner) -> R,
{
    GRAPH_BUILDER.with(|gb| {
        let mut opt = gb.borrow_mut();
        match opt.as_mut() {
            Some(builder) => Ok(f(builder)),
            None => Err(SynthDefError::NoActiveBuilder),
        }
    })
}

/// Final graph IR ready for encoding.
///
/// This is an immutable snapshot of the graph that can be encoded
/// to the scsyndef binary format.
#[derive(Clone, Debug)]
pub struct GraphIR {
    /// SynthDef name.
    pub name: String,
    /// Constant values.
    pub constants: Vec<f32>,
    /// Parameter specifications.
    pub params: Vec<ParamSpec>,
    /// UGen nodes in topological order.
    pub nodes: Vec<UGenNode>,
    /// Output bus number.
    pub out_bus: i32,
}

impl GraphIR {
    /// Create a GraphIR from a builder.
    pub fn from_builder(name: String, builder: GraphBuilderInner) -> Self {
        Self {
            name,
            constants: builder.constants,
            params: builder.params,
            nodes: builder.nodes,
            out_bus: builder.out_bus,
        }
    }

    /// Compute the total number of parameter value slots.
    pub fn total_param_slots(&self) -> usize {
        self.params.iter().map(|p| p.default.len()).sum()
    }

    /// Validate the graph structure before encoding.
    pub fn validate(&self) -> Result<()> {
        // Check that Control UGen is at index 0 if params exist
        if !self.params.is_empty() {
            if self.nodes.is_empty() {
                return Err(SynthDefError::ValidationError(
                    "Graph has parameters but no Control UGen".to_string(),
                ));
            }

            let control_node = &self.nodes[0];
            if control_node.name != "Control" {
                return Err(SynthDefError::ValidationError(format!(
                    "First UGen should be Control, got {}",
                    control_node.name
                )));
            }

            if control_node.rate != Rate::Control {
                return Err(SynthDefError::ValidationError(
                    "Control UGen must have Control rate".to_string(),
                ));
            }

            let expected_outputs = self.total_param_slots() as u32;
            if control_node.num_outputs != expected_outputs {
                return Err(SynthDefError::ValidationError(format!(
                    "Control UGen has {} outputs, expected {}",
                    control_node.num_outputs, expected_outputs
                )));
            }
        }

        // Check topological ordering: no UGen should reference a future UGen
        for (idx, node) in self.nodes.iter().enumerate() {
            for input in &node.inputs {
                if let Input::Node { node_id, .. } = input {
                    if *node_id >= idx as u32 {
                        return Err(SynthDefError::ValidationError(format!(
                            "UGen {} references future UGen {} - violates topological order",
                            idx, node_id
                        )));
                    }
                }
            }
        }

        // Check that param indices cover [0..P-1] without gaps
        let total_slots = self.total_param_slots();
        if total_slots > 0 {
            let mut covered = vec![false; total_slots];
            for param in &self.params {
                for i in 0..param.default.len() {
                    let slot = param.index + i;
                    if slot >= total_slots {
                        return Err(SynthDefError::ValidationError(format!(
                            "Parameter '{}' index {} exceeds total slots {}",
                            param.name, slot, total_slots
                        )));
                    }
                    covered[slot] = true;
                }
            }

            for (i, &is_covered) in covered.iter().enumerate() {
                if !is_covered {
                    return Err(SynthDefError::ValidationError(format!(
                        "Parameter slot {} is not covered by any parameter",
                        i
                    )));
                }
            }
        }

        // Validate all node references
        for (idx, node) in self.nodes.iter().enumerate() {
            for input in &node.inputs {
                match input {
                    Input::Constant(_) => {}
                    Input::Node {
                        node_id,
                        output_index,
                    } => {
                        if *node_id as usize >= self.nodes.len() {
                            return Err(SynthDefError::ValidationError(format!(
                                "UGen {} references invalid node {}",
                                idx, node_id
                            )));
                        }
                        let referenced_node = &self.nodes[*node_id as usize];
                        if *output_index >= referenced_node.num_outputs {
                            return Err(SynthDefError::ValidationError(format!(
                                "UGen {} references output {} of node {}, but it only has {} outputs",
                                idx, output_index, node_id, referenced_node.num_outputs
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_as_byte() {
        assert_eq!(Rate::Scalar.as_byte(), 0);
        assert_eq!(Rate::Control.as_byte(), 1);
        assert_eq!(Rate::Audio.as_byte(), 2);
    }

    #[test]
    fn test_add_constant_dedup() {
        let mut builder = GraphBuilderInner::new();
        let idx1 = builder.add_constant(440.0);
        let idx2 = builder.add_constant(440.0);
        let idx3 = builder.add_constant(880.0);
        assert_eq!(idx1, idx2);
        assert_ne!(idx1, idx3);
        assert_eq!(builder.constants.len(), 2);
    }

    #[test]
    fn test_add_param() {
        let mut builder = GraphBuilderInner::new();
        builder.add_param("freq".to_string(), vec![440.0], None);
        builder.add_param("amp".to_string(), vec![0.5], None);
        assert_eq!(builder.params.len(), 2);
        assert_eq!(builder.total_param_slots(), 2);
    }

    #[test]
    fn test_graph_ir_validation() {
        let builder = GraphBuilderInner::new();
        let ir = GraphIR::from_builder("test".to_string(), builder);
        assert!(ir.validate().is_ok());
    }
}
