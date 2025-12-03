//! SynthDef v2 binary encoder.
//!
//! This module encodes a [`GraphIR`] into the binary format understood
//! by SuperCollider's scsynth server.

use super::errors::*;
use super::graph::*;
use byteorder::{BigEndian, WriteBytesExt};
use std::io::Write;

/// Encode a GraphIR into a SynthDef v2 binary format.
///
/// The resulting bytes can be sent to scsynth via the `/d_recv` OSC command.
pub fn encode_synthdef(ir: &GraphIR) -> Result<Vec<u8>> {
    ir.validate()?;

    let mut buf = Vec::new();

    // Header: "SCgf" (4 bytes)
    buf.write_all(b"SCgf")
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write header: {}", e)))?;

    // Version: 2 (i32 big-endian)
    buf.write_i32::<BigEndian>(2)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write version: {}", e)))?;

    // Number of definitions: 1 (i16 big-endian)
    buf.write_i16::<BigEndian>(1)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write def count: {}", e)))?;

    encode_graph(&mut buf, ir)?;

    Ok(buf)
}

fn encode_graph(buf: &mut Vec<u8>, ir: &GraphIR) -> Result<()> {
    // Name (pstring: length as u8, then bytes)
    write_pstring(buf, &ir.name)?;

    // Constants (i32 count, then f32 values)
    buf.write_i32::<BigEndian>(ir.constants.len() as i32)
        .map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write constant count: {}", e))
        })?;
    for &c in &ir.constants {
        buf.write_f32::<BigEndian>(c).map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write constant: {}", e))
        })?;
    }

    // Controls/Parameters (i32 count = P total slots, then f32 values)
    let total_slots = ir.total_param_slots();
    buf.write_i32::<BigEndian>(total_slots as i32)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write param count: {}", e)))?;

    // Build the flattened paramValues[P] array
    let mut param_values = vec![0.0f32; total_slots];
    for param in &ir.params {
        for (i, &val) in param.default.iter().enumerate() {
            param_values[param.index + i] = val;
        }
    }

    for &val in &param_values {
        buf.write_f32::<BigEndian>(val).map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write param default: {}", e))
        })?;
    }

    // Parameter names (i32 count = N named params, then for each: pstring name, i32 index)
    buf.write_i32::<BigEndian>(ir.params.len() as i32)
        .map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write param name count: {}", e))
        })?;
    for param in &ir.params {
        write_pstring(buf, &param.name)?;
        buf.write_i32::<BigEndian>(param.index as i32)
            .map_err(|e| {
                SynthDefError::EncodingError(format!("Failed to write param name index: {}", e))
            })?;
    }

    // UGens (i32 count, then UGen specs)
    buf.write_i32::<BigEndian>(ir.nodes.len() as i32)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write ugen count: {}", e)))?;

    for node in &ir.nodes {
        encode_ugen(buf, node, &ir.constants)?;
    }

    // Variants: 0 (i16)
    buf.write_i16::<BigEndian>(0).map_err(|e| {
        SynthDefError::EncodingError(format!("Failed to write variant count: {}", e))
    })?;

    Ok(())
}

fn encode_ugen(buf: &mut Vec<u8>, node: &UGenNode, constants: &[f32]) -> Result<()> {
    // UGen name (pstring)
    write_pstring(buf, &node.name)?;

    // Rate (i8)
    buf.write_i8(node.rate.as_byte() as i8)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write ugen rate: {}", e)))?;

    // Number of inputs (i32)
    buf.write_i32::<BigEndian>(node.inputs.len() as i32)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write input count: {}", e)))?;

    // Number of outputs (i32)
    buf.write_i32::<BigEndian>(node.num_outputs as i32)
        .map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write output count: {}", e))
        })?;

    // Special index (i16)
    buf.write_i16::<BigEndian>(node.special_index)
        .map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write special index: {}", e))
        })?;

    // Inputs (each is i32 source index, i32 output index)
    for input in &node.inputs {
        match input {
            Input::Constant(val) => {
                buf.write_i32::<BigEndian>(-1).map_err(|e| {
                    SynthDefError::EncodingError(format!("Failed to write constant input: {}", e))
                })?;
                let const_idx = constants
                    .iter()
                    .position(|&c| (c - val).abs() < 1e-9)
                    .ok_or_else(|| {
                        SynthDefError::EncodingError(format!("Constant {} not found in table", val))
                    })?;
                buf.write_i32::<BigEndian>(const_idx as i32).map_err(|e| {
                    SynthDefError::EncodingError(format!("Failed to write constant index: {}", e))
                })?;
            }
            Input::Node {
                node_id,
                output_index,
            } => {
                buf.write_i32::<BigEndian>(*node_id as i32).map_err(|e| {
                    SynthDefError::EncodingError(format!("Failed to write node input: {}", e))
                })?;
                buf.write_i32::<BigEndian>(*output_index as i32)
                    .map_err(|e| {
                        SynthDefError::EncodingError(format!("Failed to write output index: {}", e))
                    })?;
            }
        }
    }

    // Output rates (i8 array, one per output)
    for _ in 0..node.num_outputs {
        buf.write_i8(node.rate.as_byte() as i8).map_err(|e| {
            SynthDefError::EncodingError(format!("Failed to write output rate: {}", e))
        })?;
    }

    Ok(())
}

fn write_pstring(buf: &mut Vec<u8>, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    if bytes.len() > 255 {
        return Err(SynthDefError::EncodingError(format!(
            "String too long for pstring: {}",
            s
        )));
    }
    buf.write_u8(bytes.len() as u8).map_err(|e| {
        SynthDefError::EncodingError(format!("Failed to write string length: {}", e))
    })?;
    buf.write_all(bytes)
        .map_err(|e| SynthDefError::EncodingError(format!("Failed to write string: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_empty_synthdef() {
        let builder = GraphBuilderInner::new();
        let ir = GraphIR::from_builder("empty".to_string(), builder);
        let bytes = encode_synthdef(&ir).unwrap();

        // Check header
        assert_eq!(&bytes[0..4], b"SCgf");
        // Check version (big-endian 2)
        assert_eq!(bytes[4..8], [0, 0, 0, 2]);
    }

    #[test]
    fn test_pstring_encoding() {
        let mut buf = Vec::new();
        write_pstring(&mut buf, "test").unwrap();
        assert_eq!(buf, vec![4, b't', b'e', b's', b't']);
    }

    #[test]
    fn test_pstring_too_long() {
        let mut buf = Vec::new();
        let long_string = "a".repeat(256);
        let result = write_pstring(&mut buf, &long_string);
        assert!(result.is_err());
    }
}
