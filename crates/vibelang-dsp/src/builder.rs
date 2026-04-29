//! SynthDef builder API for Rhai.
//!
//! This module provides the `SynthDef` builder that allows creating
//! SuperCollider SynthDefs from Rhai closures.

use crate::encoder::encode_synthdef;
use crate::errors::{Result, SynthDefError};
use crate::graph::{
    clear_active_builder, set_active_builder, GraphBuilderInner, GraphIR, Input, Rate,
};
use crate::helpers;
use crate::rhainodes::{self, NodeRef};
use crate::ugens::register_generated_ugens;

/// Selects how a body closure receives its parameters.
///
/// `Positional` passes one argument per declared param (capped at 10/11 by
/// Rhai's `IntoFuncArgs` tuple impls). `Map` passes a single
/// `rhai::Map<Identifier, Dynamic>` keyed by param name, sidestepping the cap.
#[derive(Clone, Copy, Debug)]
enum BodyDispatch {
    Positional,
    Map,
}

/// A named output port on a synthdef.
///
/// Channel count is usually 1 for CV/mono ports and 2 for the legacy stereo
/// `out` port. Routing/codegen for multiple ports is handled by later stories;
/// this descriptor just records the port shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputPort {
    pub name: String,
    pub channels: u8,
}

/// SynthDef builder.
#[derive(Clone, Debug)]
pub struct SynthDef {
    pub name: String,
    pub params: Vec<(String, f32, Option<f32>)>, // (name, default, lag_ms)
    pub out_bus_tag: Option<String>,
    pub outputs: Vec<OutputPort>,
    // True once the user has called `.output(...)` at least once. Tracks whether
    // `outputs` is the implicit legacy default or an explicit declaration.
    outputs_explicit: bool,
}

impl SynthDef {
    /// Create a new SynthDef with the given name.
    pub fn new(name: String) -> Self {
        Self {
            name,
            params: Vec::new(),
            out_bus_tag: None,
            outputs: vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
            }],
            outputs_explicit: false,
        }
    }

    /// Declare a named output port. Channels typically 1 (CV/mono) or 2 (stereo).
    ///
    /// The first call replaces the implicit legacy `out` port; subsequent calls
    /// append. Empty names and duplicate names are rejected.
    pub fn output(&mut self, name: String, channels: u8) -> Result<&mut Self> {
        if name.is_empty() {
            return Err(SynthDefError::ValidationError(
                "Output port name cannot be empty".to_string(),
            ));
        }
        if !self.outputs_explicit {
            self.outputs.clear();
            self.outputs_explicit = true;
        }
        if self.outputs.iter().any(|p| p.name == name) {
            return Err(SynthDefError::ValidationError(format!(
                "Duplicate output port name: {}",
                name
            )));
        }
        self.outputs.push(OutputPort { name, channels });
        Ok(self)
    }

    /// Add a float parameter.
    pub fn arg_f(&mut self, name: String, default: f64) -> &mut Self {
        self.params.push((name, default as f32, None));
        self
    }

    /// Set glide/lag time for a parameter in milliseconds.
    pub fn glide_ms(&mut self, name: String, ms: f64) -> &mut Self {
        // Find the param and set its lag
        for param in &mut self.params {
            if param.0 == name {
                param.2 = Some(ms as f32);
                return self;
            }
        }
        // If param doesn't exist, add it with the lag
        self.params.push((name, 0.0, Some(ms as f32)));
        self
    }

    /// Set the output bus tag.
    pub fn out_bus(&mut self, tag: String) -> &mut Self {
        self.out_bus_tag = Some(tag);
        self
    }

    /// Execute the body using a Rhai closure.
    /// The closure receives the parameters as arguments (as NodeRefs).
    pub fn build_body_closure(self, closure: rhai::FnPtr) -> Result<GraphIR> {
        self.build_body_closure_with_options(closure, true)
    }

    /// Execute an FX body closure.
    ///
    /// The closure must accept `input` as its first argument (an array of NodeRefs)
    /// and return the processed channels as an array of NodeRefs. All routing
    /// to ReplaceOut is handled automatically so FX authors never touch buses.
    pub fn build_effect_closure(
        self,
        closure: rhai::FnPtr,
        num_channels: usize,
    ) -> Result<GraphIR> {
        self.build_effect_closure_inner(closure, num_channels, BodyDispatch::Positional)
    }

    /// Execute an FX body closure that takes a single `Map` of params.
    ///
    /// The map contains the user-declared params plus an `input` key holding
    /// the channel array. Sidesteps Rhai's tuple-arity cap so FX bodies can
    /// declare more than 10 user parameters.
    pub fn build_effect_map_closure(
        self,
        closure: rhai::FnPtr,
        num_channels: usize,
    ) -> Result<GraphIR> {
        self.build_effect_closure_inner(closure, num_channels, BodyDispatch::Map)
    }

    fn build_effect_closure_inner(
        self,
        closure: rhai::FnPtr,
        num_channels: usize,
        dispatch: BodyDispatch,
    ) -> Result<GraphIR> {
        if num_channels == 0 {
            return Err(SynthDefError::ValidationError(
                "Effects must declare at least one channel".to_string(),
            ));
        }

        // Create a new graph builder
        let mut builder = GraphBuilderInner::new();

        let params = self.params.clone();

        // Hidden routing params come first so we can reference them predictably
        builder.add_param("__fx_bus_in".to_string(), vec![0.0], None);
        builder.add_param("__fx_bus_out".to_string(), vec![0.0], None);

        // Add user parameters
        for (name, default, lag_ms) in &params {
            builder.add_param(name.clone(), vec![*default], *lag_ms);
        }

        builder.create_control_ugen();

        // Helper to get encoded NodeRefs for parameters
        let param_ref = |builder: &GraphBuilderInner, name: &str| -> Result<NodeRef> {
            builder
                .params
                .iter()
                .find(|p| p.name == name)
                .map(|spec| NodeRef(0xFFFFFFFF - spec.index as u32))
                .ok_or_else(|| {
                    SynthDefError::ValidationError(format!("Missing internal FX param {}", name))
                })
        };

        let bus_in_ref = param_ref(&builder, "__fx_bus_in")?;
        let bus_out_ref = param_ref(&builder, "__fx_bus_out")?;

        // Build NodeRefs for user parameters (skip hidden ones)
        let mut param_nodes = Vec::new();
        for (name, _, _) in &params {
            let spec = builder
                .params
                .iter()
                .find(|p| p.name == *name)
                .ok_or_else(|| {
                    SynthDefError::ValidationError(format!("Missing FX param {}", name))
                })?;
            param_nodes.push(rhai::Dynamic::from(NodeRef(0xFFFFFFFF - spec.index as u32)));
        }

        // Activate builder and construct the fixed In.ar input
        set_active_builder(builder);
        let input_array = match helpers::in_ar_n(bus_in_ref, num_channels as f64) {
            Ok(arr) => arr,
            Err(err) => {
                clear_active_builder();
                return Err(err);
            }
        };
        let expected_channels = input_array.len();

        // Create engine with DSP components
        let mut engine = rhai::Engine::new();
        rhainodes::register_node_ref(&mut engine);
        register_generated_ugens(&mut engine);
        helpers::register_helpers(&mut engine);
        engine.register_type::<GraphIR>();

        // Prepare arguments (input first, followed by user params)
        let mut args: Vec<rhai::Dynamic> = vec![rhai::Dynamic::from(input_array.clone())];
        args.extend(param_nodes.iter().cloned());

        let empty_ast = rhai::AST::empty();
        let call_result = match dispatch {
            BodyDispatch::Map => {
                let mut map = rhai::Map::new();
                map.insert("input".into(), rhai::Dynamic::from(input_array.clone()));
                for ((name, _, _), node) in params.iter().zip(param_nodes.iter()) {
                    map.insert(name.as_str().into(), node.clone());
                }
                closure.call(&engine, &empty_ast, (rhai::Dynamic::from(map),))
            }
            BodyDispatch::Positional => match args.len() {
            1 => closure.call(&engine, &empty_ast, (args[0].clone(),)),
            2 => closure.call(&engine, &empty_ast, (args[0].clone(), args[1].clone())),
            3 => closure.call(
                &engine,
                &empty_ast,
                (args[0].clone(), args[1].clone(), args[2].clone()),
            ),
            4 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                ),
            ),
            5 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                ),
            ),
            6 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                ),
            ),
            7 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                    args[6].clone(),
                ),
            ),
            8 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                    args[6].clone(),
                    args[7].clone(),
                ),
            ),
            9 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                    args[6].clone(),
                    args[7].clone(),
                    args[8].clone(),
                ),
            ),
            10 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                    args[6].clone(),
                    args[7].clone(),
                    args[8].clone(),
                    args[9].clone(),
                ),
            ),
            11 => closure.call(
                &engine,
                &empty_ast,
                (
                    args[0].clone(),
                    args[1].clone(),
                    args[2].clone(),
                    args[3].clone(),
                    args[4].clone(),
                    args[5].clone(),
                    args[6].clone(),
                    args[7].clone(),
                    args[8].clone(),
                    args[9].clone(),
                    args[10].clone(),
                ),
            ),
            _ => {
                clear_active_builder();
                return Err(SynthDefError::RhaiError(
                    "Too many FX parameters (max 10 user params); use .body_map for higher arities".to_string(),
                ));
            }
            },
        };

        let result: rhai::Dynamic = match call_result {
            Ok(val) => val,
            Err(e) => {
                clear_active_builder();
                return Err(SynthDefError::RhaiError(format!("FX body error: {}", e)));
            }
        };

        // Normalize return value into an array of NodeRefs
        let mut output_channels = if result.is::<rhai::Array>() {
            result.cast::<rhai::Array>()
        } else if result.is::<NodeRef>() {
            let node = result.cast::<NodeRef>();
            vec![rhai::Dynamic::from(node)]
        } else {
            clear_active_builder();
            return Err(SynthDefError::InvalidBodyReturn);
        };

        if output_channels.len() != expected_channels {
            clear_active_builder();
            return Err(SynthDefError::ValidationError(format!(
                "FX returned {} channel(s), but the group provides {}",
                output_channels.len(),
                expected_channels
            )));
        }

        // Ensure every channel is a NodeRef
        for ch in &mut output_channels {
            if ch.clone().try_cast::<NodeRef>().is_none() {
                clear_active_builder();
                return Err(SynthDefError::InvalidBodyReturn);
            }
        }

        if let Err(e) = helpers::replace_out_ar_n(bus_out_ref, output_channels.clone()) {
            clear_active_builder();
            return Err(e);
        }

        let builder = clear_active_builder().ok_or(SynthDefError::NoActiveBuilder)?;

        Ok(GraphIR::from_builder(self.name, builder))
    }

    /// Execute the body using a Rhai closure with options.
    ///
    /// # Arguments
    /// * `closure` - The Rhai closure defining the synthdef body
    /// * `add_out_node` - If true, automatically adds an Out node at the end (for voices).
    ///   If false, no Out node is added (for effects that use ReplaceOut).
    pub fn build_body_closure_with_options(
        self,
        closure: rhai::FnPtr,
        add_out_node: bool,
    ) -> Result<GraphIR> {
        self.build_body_closure_with_options_inner(closure, add_out_node, BodyDispatch::Positional)
    }

    /// Execute the body using a Rhai closure that takes a single `Map` of params.
    ///
    /// Sidesteps Rhai's tuple-arity cap so synthdef bodies can declare more
    /// than 10 parameters. The closure receives a single `Map<String, Dynamic>`
    /// keyed by the user-declared param names.
    pub fn build_body_map_closure_with_options(
        self,
        closure: rhai::FnPtr,
        add_out_node: bool,
    ) -> Result<GraphIR> {
        self.build_body_closure_with_options_inner(closure, add_out_node, BodyDispatch::Map)
    }

    fn build_body_closure_with_options_inner(
        self,
        closure: rhai::FnPtr,
        add_out_node: bool,
        dispatch: BodyDispatch,
    ) -> Result<GraphIR> {
        // Create a new graph builder
        let mut builder = GraphBuilderInner::new();

        // Use explicitly declared parameters
        let params = self.params.clone();

        // Check if "out" parameter exists, if not add it automatically when add_out_node is true
        let has_out_param = params.iter().any(|(name, _, _)| name == "out");

        // Add all parameters first (as single-value arrays)
        for (name, default, lag_ms) in &params {
            builder.add_param(name.clone(), vec![*default], *lag_ms);
        }

        // Add automatic "out" parameter if not explicitly defined and we're adding an Out node
        if add_out_node && !has_out_param {
            builder.add_param("out".to_string(), vec![0.0], None);
        }

        // Create the Control UGen node for parameters (must be first node, at index 0)
        builder.create_control_ugen();

        // Create NodeRefs for parameters - all reference Control UGen (node 0) with different output indices
        // We encode this by using a special value: node_id = 0xFFFFFFFF - slot_index
        // The slot_index is the parameter's index in the flattened parameter array, not its array index
        // This will be handled specially when converting to Input::Node
        let mut param_nodes = Vec::new();
        for (i, _) in params.iter().enumerate() {
            // Get the slot index from the ParamSpec (not the array index i)
            let slot_index = builder.params[i].index;
            // Use a special encoding: 0xFFFFFFFF - slot_index to indicate this is a parameter
            param_nodes.push(rhai::Dynamic::from(NodeRef(0xFFFFFFFF - slot_index as u32)));
        }

        // Set as active
        set_active_builder(builder);

        // Create engine with DSP components (same as main engine, but without API)
        // This ensures UGen functions with defaults are available in SynthDef bodies
        let mut engine = rhai::Engine::new();
        rhainodes::register_node_ref(&mut engine);
        // Register UGen functions with default parameters
        register_generated_ugens(&mut engine);
        helpers::register_helpers(&mut engine);
        engine.register_type::<GraphIR>();

        // Create empty AST for closure call (no wrapper AST needed anymore)
        let empty_ast = rhai::AST::empty();

        // Call closure - UGen functions are already registered on the engine with defaults
        let result: rhai::Dynamic = match dispatch {
            BodyDispatch::Map => {
                let mut map = rhai::Map::new();
                for ((name, _, _), node) in params.iter().zip(param_nodes.iter()) {
                    map.insert(name.as_str().into(), node.clone());
                }
                closure.call(&engine, &empty_ast, (rhai::Dynamic::from(map),))
            }
            BodyDispatch::Positional => match param_nodes.len() {
            0 => closure.call(&engine, &empty_ast, ()),
            1 => closure.call(&engine, &empty_ast, (param_nodes[0].clone(),)),
            2 => closure.call(
                &engine,
                &empty_ast,
                (param_nodes[0].clone(), param_nodes[1].clone()),
            ),
            3 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                ),
            ),
            4 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                ),
            ),
            5 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                ),
            ),
            6 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                ),
            ),
            7 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                ),
            ),
            8 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                ),
            ),
            9 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                    param_nodes[8].clone(),
                ),
            ),
            10 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                    param_nodes[8].clone(),
                    param_nodes[9].clone(),
                ),
            ),
            _ => {
                clear_active_builder();
                return Err(SynthDefError::RhaiError(
                    "Too many parameters (max 10); use .body_map for higher arities".to_string(),
                ));
            }
            },
        }
        .map_err(|e| {
            clear_active_builder();
            SynthDefError::RhaiError(format!("Body closure error: {}", e))
        })?;

        // Get the result - can be either a single NodeRef (mono) or an Array of NodeRefs (stereo/multi-channel)
        enum BodyResult {
            Mono(NodeRef),
            MultiChannel(Vec<NodeRef>),
        }

        let body_result = if let Some(node) = result.clone().try_cast::<NodeRef>() {
            // Single NodeRef - mono signal
            BodyResult::Mono(node)
        } else if let Some(arr) = result.clone().try_cast::<rhai::Array>() {
            // Array of NodeRefs - multi-channel signal
            let mut channels = Vec::new();
            for (i, item) in arr.iter().enumerate() {
                if let Some(node) = item.clone().try_cast::<NodeRef>() {
                    channels.push(node);
                } else {
                    clear_active_builder();
                    return Err(SynthDefError::ValidationError(format!(
                        "Body returned array with non-NodeRef at index {} (got {})",
                        i,
                        item.type_name()
                    )));
                }
            }
            if channels.is_empty() {
                clear_active_builder();
                return Err(SynthDefError::ValidationError(
                    "Body returned empty array".to_string(),
                ));
            }
            BodyResult::MultiChannel(channels)
        } else {
            clear_active_builder();
            return Err(SynthDefError::ValidationError(format!(
                "Body must return a signal (NodeRef) or array of signals [left, right], got {}",
                result.type_name()
            )));
        };

        // Get the builder back
        let mut builder = clear_active_builder().ok_or(SynthDefError::NoActiveBuilder)?;

        // Conditionally add an Out node
        if add_out_node {
            // Add an Out node using the "out" parameter
            // Find the "out" parameter index
            let out_param_idx = builder
                .params
                .iter()
                .position(|p| p.name == "out")
                .expect("'out' parameter should exist");

            match body_result {
                BodyResult::Mono(result_node) => {
                    // Check if result is mono (1 output) or already stereo (2+ outputs)
                    let result_node_id = result_node.id();
                    let result_num_outputs = if (result_node_id as usize) < builder.nodes.len() {
                        builder.nodes[result_node_id as usize].num_outputs
                    } else {
                        1 // Default to mono if we can't determine
                    };

                    if result_num_outputs == 1 {
                        // Mono signal - use Pan2 to convert to stereo (centered)
                        builder.add_constant(0.0); // center position
                        builder.add_constant(1.0); // full level

                        let pan2_inputs = vec![
                            Input::Node {
                                node_id: result_node_id,
                                output_index: 0,
                            },
                            Input::Constant(0.0), // pos = center
                            Input::Constant(1.0), // level = 1.0
                        ];
                        let pan2_node =
                            builder.add_node("Pan2".to_string(), Rate::Audio, pan2_inputs, 2, 0);

                        let out_inputs = vec![
                            Input::Node {
                                node_id: 0,
                                output_index: out_param_idx as u32,
                            },
                            Input::Node {
                                node_id: pan2_node.id(),
                                output_index: 0,
                            },
                            Input::Node {
                                node_id: pan2_node.id(),
                                output_index: 1,
                            },
                        ];
                        builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);
                    } else {
                        // Already stereo (or more) from a multi-output UGen - output directly
                        let mut out_inputs = vec![Input::Node {
                            node_id: 0,
                            output_index: out_param_idx as u32,
                        }];
                        for i in 0..result_num_outputs {
                            out_inputs.push(Input::Node {
                                node_id: result_node_id,
                                output_index: i,
                            });
                        }
                        builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);
                    }
                }
                BodyResult::MultiChannel(channels) => {
                    // Explicit multi-channel return (e.g., [left, right])
                    // Output each channel directly without Pan2 wrapping
                    let mut out_inputs = vec![Input::Node {
                        node_id: 0,
                        output_index: out_param_idx as u32,
                    }];

                    for channel_node in &channels {
                        // Use to_input() which handles parameter refs correctly
                        out_inputs.push(channel_node.to_input());
                    }

                    builder.add_node("Out".to_string(), Rate::Audio, out_inputs, 0, 0);
                }
            }
        }

        // Convert to GraphIR
        let ir = GraphIR::from_builder(self.name, builder);

        Ok(ir)
    }

    /// Build the synthdef and return the encoded bytes.
    pub fn build_and_encode(self, closure: rhai::FnPtr) -> Result<Vec<u8>> {
        let ir = self.build_body_closure(closure)?;
        encode_synthdef(&ir)
    }

    /// Build an effect synthdef and return the encoded bytes.
    pub fn build_effect_and_encode(
        self,
        closure: rhai::FnPtr,
        num_channels: usize,
    ) -> Result<Vec<u8>> {
        let ir = self.build_effect_closure(closure, num_channels)?;
        encode_synthdef(&ir)
    }

    /// Execute a modulator body closure.
    ///
    /// Modulators are control-rate synthdefs that output to control buses.
    /// The closure should return a single NodeRef representing the control signal.
    /// An "out" parameter is automatically added for the control bus number.
    pub fn build_modulator_closure(self, closure: rhai::FnPtr) -> Result<GraphIR> {
        self.build_modulator_closure_inner(closure, BodyDispatch::Positional)
    }

    /// Execute a modulator body closure that takes a single `Map` of params.
    ///
    /// Sidesteps Rhai's tuple-arity cap so modulators can declare more than
    /// 10 parameters. The closure receives a single `Map<String, Dynamic>`
    /// keyed by the user-declared param names.
    pub fn build_modulator_map_closure(self, closure: rhai::FnPtr) -> Result<GraphIR> {
        self.build_modulator_closure_inner(closure, BodyDispatch::Map)
    }

    fn build_modulator_closure_inner(
        self,
        closure: rhai::FnPtr,
        dispatch: BodyDispatch,
    ) -> Result<GraphIR> {
        // Create a new graph builder
        let mut builder = GraphBuilderInner::new();

        // Use explicitly declared parameters
        let params = self.params.clone();

        // Check if "out" parameter exists, if not add it automatically
        let has_out_param = params.iter().any(|(name, _, _)| name == "out");

        // Add all parameters first (as single-value arrays)
        for (name, default, lag_ms) in &params {
            builder.add_param(name.clone(), vec![*default], *lag_ms);
        }

        // Add automatic "out" parameter if not explicitly defined
        if !has_out_param {
            builder.add_param("out".to_string(), vec![0.0], None);
        }

        // Create the Control UGen node for parameters (must be first node, at index 0)
        builder.create_control_ugen();

        // Create NodeRefs for parameters
        let mut param_nodes = Vec::new();
        for (i, _) in params.iter().enumerate() {
            let slot_index = builder.params[i].index;
            param_nodes.push(rhai::Dynamic::from(NodeRef(0xFFFFFFFF - slot_index as u32)));
        }

        // Set as active
        set_active_builder(builder);

        // Create engine with DSP components
        let mut engine = rhai::Engine::new();
        rhainodes::register_node_ref(&mut engine);
        register_generated_ugens(&mut engine);
        helpers::register_helpers(&mut engine);
        engine.register_type::<GraphIR>();

        // Create empty AST for closure call
        let empty_ast = rhai::AST::empty();

        // Call closure with parameters
        let result: rhai::Dynamic = match dispatch {
            BodyDispatch::Map => {
                let mut map = rhai::Map::new();
                for ((name, _, _), node) in params.iter().zip(param_nodes.iter()) {
                    map.insert(name.as_str().into(), node.clone());
                }
                closure.call(&engine, &empty_ast, (rhai::Dynamic::from(map),))
            }
            BodyDispatch::Positional => match param_nodes.len() {
            0 => closure.call(&engine, &empty_ast, ()),
            1 => closure.call(&engine, &empty_ast, (param_nodes[0].clone(),)),
            2 => closure.call(
                &engine,
                &empty_ast,
                (param_nodes[0].clone(), param_nodes[1].clone()),
            ),
            3 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                ),
            ),
            4 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                ),
            ),
            5 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                ),
            ),
            6 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                ),
            ),
            7 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                ),
            ),
            8 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                ),
            ),
            9 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                    param_nodes[8].clone(),
                ),
            ),
            10 => closure.call(
                &engine,
                &empty_ast,
                (
                    param_nodes[0].clone(),
                    param_nodes[1].clone(),
                    param_nodes[2].clone(),
                    param_nodes[3].clone(),
                    param_nodes[4].clone(),
                    param_nodes[5].clone(),
                    param_nodes[6].clone(),
                    param_nodes[7].clone(),
                    param_nodes[8].clone(),
                    param_nodes[9].clone(),
                ),
            ),
            _ => {
                clear_active_builder();
                return Err(SynthDefError::RhaiError(
                    "Too many parameters (max 10); use .body_map for higher arities".to_string(),
                ));
            }
            },
        }
        .map_err(|e| {
            clear_active_builder();
            SynthDefError::RhaiError(format!("Modulator body closure error: {}", e))
        })?;

        // Get the result - must be a single NodeRef (control signal)
        let result_node = if let Some(node) = result.clone().try_cast::<NodeRef>() {
            node
        } else {
            clear_active_builder();
            return Err(SynthDefError::ValidationError(format!(
                "Modulator body must return a single control signal (NodeRef), got {}",
                result.type_name()
            )));
        };

        // Get the builder back
        let mut builder = clear_active_builder().ok_or(SynthDefError::NoActiveBuilder)?;

        // Find the "out" parameter index
        let out_param_idx = builder
            .params
            .iter()
            .position(|p| p.name == "out")
            .expect("'out' parameter should exist");

        // Add Out.kr node for control-rate output to control bus
        let out_inputs = vec![
            Input::Node {
                node_id: 0,
                output_index: out_param_idx as u32,
            },
            result_node.to_input(),
        ];
        builder.add_node("Out".to_string(), Rate::Control, out_inputs, 0, 0);

        // Convert to GraphIR
        let ir = GraphIR::from_builder(self.name, builder);

        Ok(ir)
    }

    /// Build a modulator synthdef and return the encoded bytes.
    pub fn build_modulator_and_encode(self, closure: rhai::FnPtr) -> Result<Vec<u8>> {
        let ir = self.build_modulator_closure(closure)?;
        encode_synthdef(&ir)
    }
}

/// Get default parameter values from a GraphIR.
pub fn get_param_defaults(ir: &GraphIR) -> std::collections::HashMap<String, f32> {
    let mut defaults = std::collections::HashMap::new();
    for param in &ir.params {
        if param.default.len() == 1 {
            defaults.insert(param.name.clone(), param.default[0]);
        }
    }
    defaults
}

#[cfg(test)]
mod body_map_tests {
    use super::*;
    use rhai::{Engine, FnPtr};

    fn parser_engine() -> Engine {
        // Default expr depth (32 in debug) is too low for arity-50 sums.
        let mut engine = Engine::new();
        engine.set_max_expr_depths(256, 256);
        engine
    }

    fn body_map_closure(arity: usize) -> FnPtr {
        let body = if arity == 0 {
            "sin_osc_ar(440.0, 0.0)".to_string()
        } else {
            let sum = (0..arity)
                .map(|i| format!("p.p{}", i))
                .collect::<Vec<_>>()
                .join(" + ");
            format!("sin_osc_ar({}, 0.0)", sum)
        };
        let script = format!("|p| {}", body);
        parser_engine()
            .eval::<FnPtr>(&script)
            .expect("parse body_map closure")
    }

    fn build_map_synthdef(name: &str, arity: usize) -> Result<GraphIR> {
        let mut sd = SynthDef::new(name.to_string());
        for i in 0..arity {
            sd.arg_f(format!("p{}", i), (i + 1) as f64);
        }
        sd.build_body_map_closure_with_options(body_map_closure(arity), true)
    }

    #[test]
    fn body_map_arity_1() {
        let ir = build_map_synthdef("body_map_arity_1", 1).expect("build arity 1");
        // 1 user param + auto-added "out" param
        assert_eq!(ir.params.len(), 2);
        assert_eq!(ir.params[0].name, "p0");
        assert_eq!(ir.params[1].name, "out");
    }

    #[test]
    fn body_map_arity_5() {
        let ir = build_map_synthdef("body_map_arity_5", 5).expect("build arity 5");
        assert_eq!(ir.params.len(), 6);
        for i in 0..5 {
            assert_eq!(ir.params[i].name, format!("p{}", i));
        }
    }

    #[test]
    fn body_map_arity_11() {
        // The whole point of body_map: this would fail under positional `body`.
        let ir = build_map_synthdef("body_map_arity_11", 11).expect("build arity 11");
        assert_eq!(ir.params.len(), 12);
    }

    #[test]
    fn body_map_arity_20() {
        let ir = build_map_synthdef("body_map_arity_20", 20).expect("build arity 20");
        assert_eq!(ir.params.len(), 21);
    }

    #[test]
    fn body_map_arity_50() {
        let ir = build_map_synthdef("body_map_arity_50", 50).expect("build arity 50");
        assert_eq!(ir.params.len(), 51);
    }

    #[test]
    fn output_default_port_when_no_explicit_call() {
        let sd = SynthDef::new("default_only".to_string());
        assert_eq!(
            sd.outputs,
            vec![OutputPort {
                name: "out".to_string(),
                channels: 2,
            }]
        );
    }

    #[test]
    fn output_single_explicit_replaces_default() {
        let mut sd = SynthDef::new("single".to_string());
        sd.output("sine".to_string(), 1).expect("first output");
        assert_eq!(
            sd.outputs,
            vec![OutputPort {
                name: "sine".to_string(),
                channels: 1,
            }]
        );
    }

    #[test]
    fn output_multi_round_trip() {
        let mut sd = SynthDef::new("multi".to_string());
        sd.output("a".to_string(), 1).expect("a");
        sd.output("b".to_string(), 2).expect("b");
        assert_eq!(
            sd.outputs,
            vec![
                OutputPort {
                    name: "a".to_string(),
                    channels: 1,
                },
                OutputPort {
                    name: "b".to_string(),
                    channels: 2,
                },
            ]
        );
    }

    #[test]
    fn output_duplicate_name_errors() {
        let mut sd = SynthDef::new("dup".to_string());
        sd.output("a".to_string(), 1).expect("first");
        let err = sd
            .output("a".to_string(), 2)
            .expect_err("duplicate must error");
        match err {
            SynthDefError::ValidationError(msg) => {
                assert!(msg.contains("Duplicate"), "msg = {}", msg);
                assert!(msg.contains("a"), "msg = {}", msg);
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn output_empty_name_errors() {
        let mut sd = SynthDef::new("empty".to_string());
        let err = sd
            .output(String::new(), 1)
            .expect_err("empty name must error");
        match err {
            SynthDefError::ValidationError(msg) => {
                assert!(msg.contains("empty"), "msg = {}", msg);
            }
            other => panic!("expected ValidationError, got {:?}", other),
        }
    }

    #[test]
    fn body_map_matches_positional_at_arity_5() {
        // Equivalent positional and map synthdefs should yield the same param
        // list, node count, and constants.
        let mut sd_pos = SynthDef::new("eq_positional".to_string());
        for i in 0..5 {
            sd_pos.arg_f(format!("p{}", i), (i + 1) as f64);
        }
        let pos_closure: FnPtr = parser_engine()
            .eval(
                "|p0, p1, p2, p3, p4| sin_osc_ar(p0 + p1 + p2 + p3 + p4, 0.0)",
            )
            .expect("parse positional closure");
        let pos_ir = sd_pos
            .build_body_closure_with_options(pos_closure, true)
            .expect("build positional");

        let map_ir = build_map_synthdef("eq_map", 5).expect("build map");

        assert_eq!(pos_ir.params.len(), map_ir.params.len());
        for (a, b) in pos_ir.params.iter().zip(map_ir.params.iter()) {
            assert_eq!(a.name, b.name, "param name mismatch");
            assert_eq!(a.default, b.default, "param default mismatch");
        }
        assert_eq!(
            pos_ir.nodes.len(),
            map_ir.nodes.len(),
            "node count differs"
        );
        assert_eq!(
            pos_ir.constants, map_ir.constants,
            "constants differ"
        );
    }
}
