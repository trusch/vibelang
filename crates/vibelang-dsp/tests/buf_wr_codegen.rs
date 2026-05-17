use rhai::{Dynamic, Engine};
use vibelang_dsp::{
    clear_active_builder, encode_synthdef, register_dsp_api, set_active_builder, GraphBuilderInner,
    GraphIR, Input, Rate,
};

#[derive(Debug)]
struct RawUgen {
    name: String,
    rate: u8,
    inputs: Vec<(i32, i32)>,
    num_outputs: i32,
    special_index: i16,
    output_rates: Vec<u8>,
}

fn read_u8(bytes: &[u8], off: &mut usize) -> u8 {
    let value = bytes[*off];
    *off += 1;
    value
}

fn read_i8(bytes: &[u8], off: &mut usize) -> i8 {
    read_u8(bytes, off) as i8
}

fn read_i16(bytes: &[u8], off: &mut usize) -> i16 {
    let value = i16::from_be_bytes(bytes[*off..*off + 2].try_into().unwrap());
    *off += 2;
    value
}

fn read_i32(bytes: &[u8], off: &mut usize) -> i32 {
    let value = i32::from_be_bytes(bytes[*off..*off + 4].try_into().unwrap());
    *off += 4;
    value
}

fn read_pstring(bytes: &[u8], off: &mut usize) -> String {
    let len = read_u8(bytes, off) as usize;
    let value = std::str::from_utf8(&bytes[*off..*off + len])
        .expect("valid pstring")
        .to_string();
    *off += len;
    value
}

fn parse_ugens(bytes: &[u8]) -> Vec<RawUgen> {
    assert_eq!(&bytes[0..4], b"SCgf");
    let mut off = 4;
    assert_eq!(read_i32(bytes, &mut off), 2);
    assert_eq!(read_i16(bytes, &mut off), 1);

    let _name = read_pstring(bytes, &mut off);

    let constant_count = read_i32(bytes, &mut off) as usize;
    off += constant_count * 4;

    let param_value_count = read_i32(bytes, &mut off) as usize;
    off += param_value_count * 4;

    let param_name_count = read_i32(bytes, &mut off) as usize;
    for _ in 0..param_name_count {
        let _ = read_pstring(bytes, &mut off);
        let _ = read_i32(bytes, &mut off);
    }

    let ugen_count = read_i32(bytes, &mut off) as usize;
    let mut ugens = Vec::with_capacity(ugen_count);
    for _ in 0..ugen_count {
        let name = read_pstring(bytes, &mut off);
        let rate = read_i8(bytes, &mut off) as u8;
        let input_count = read_i32(bytes, &mut off) as usize;
        let num_outputs = read_i32(bytes, &mut off);
        let special_index = read_i16(bytes, &mut off);

        let mut inputs = Vec::with_capacity(input_count);
        for _ in 0..input_count {
            inputs.push((read_i32(bytes, &mut off), read_i32(bytes, &mut off)));
        }

        let mut output_rates = Vec::with_capacity(num_outputs as usize);
        for _ in 0..num_outputs {
            output_rates.push(read_i8(bytes, &mut off) as u8);
        }

        ugens.push(RawUgen {
            name,
            rate,
            inputs,
            num_outputs,
            special_index,
            output_rates,
        });
    }

    assert_eq!(read_i16(bytes, &mut off), 0);
    assert_eq!(off, bytes.len());
    ugens
}

fn build_via_rhai(script: &str) -> GraphIR {
    let mut engine = Engine::new();
    register_dsp_api(&mut engine);

    set_active_builder(GraphBuilderInner::new());
    let _: Dynamic = engine
        .eval(script)
        .unwrap_or_else(|e| panic!("rhai eval failed for {script:?}: {e}"));
    let builder = clear_active_builder().expect("active builder vanished");
    GraphIR::from_builder("buf_wr_codegen".to_string(), builder)
}

#[test]
fn buf_wr_kr_encodes_server_input_order_and_zero_outputs() {
    let ir = build_via_rhai("buf_wr_kr(dc_kr(0.5), 37, 0.0, 1);");
    let node = ir
        .nodes
        .iter()
        .find(|node| node.name == "BufWr")
        .expect("BufWr node missing");

    assert_eq!(node.rate, Rate::Control);
    assert_eq!(node.num_outputs, 0, "BufWr is side-effect-only");
    assert_eq!(node.special_index, 0);
    assert_eq!(node.inputs.len(), 4);

    assert!(matches!(node.inputs[0], Input::Constant(v) if (v - 37.0).abs() < f32::EPSILON));
    assert!(matches!(node.inputs[1], Input::Constant(v) if v.abs() < f32::EPSILON));
    assert!(matches!(node.inputs[2], Input::Constant(v) if (v - 1.0).abs() < f32::EPSILON));
    assert!(
        matches!(node.inputs[3], Input::Node { .. }),
        "write signal must be encoded after bufnum/phase/loop"
    );

    let bytes = encode_synthdef(&ir).expect("encode succeeds");
    let raw = parse_ugens(&bytes)
        .into_iter()
        .find(|ugen| ugen.name == "BufWr")
        .expect("encoded BufWr missing");

    assert_eq!(raw.rate, Rate::Control.as_byte());
    assert_eq!(raw.inputs.len(), 4);
    assert_eq!(raw.num_outputs, 0);
    assert_eq!(raw.special_index, 0);
    assert!(raw.output_rates.is_empty());
    assert_eq!(raw.inputs[0], (-1, 1), "bufnum constant comes first");
    assert_eq!(raw.inputs[1], (-1, 2), "phase constant comes second");
    assert_eq!(raw.inputs[2], (-1, 3), "loop constant comes third");
}

#[test]
fn buf_wr_ar_appends_multichannel_input_array_after_write_controls() {
    let ir = build_via_rhai("buf_wr_ar([dc_ar(0.25), dc_ar(0.5)], 12, 3.0, 0);");
    let node = ir
        .nodes
        .iter()
        .find(|node| node.name == "BufWr")
        .expect("BufWr node missing");

    assert_eq!(node.rate, Rate::Audio);
    assert_eq!(node.num_outputs, 0);
    assert_eq!(node.inputs.len(), 5);
    assert!(matches!(node.inputs[0], Input::Constant(v) if (v - 12.0).abs() < f32::EPSILON));
    assert!(matches!(node.inputs[1], Input::Constant(v) if (v - 3.0).abs() < f32::EPSILON));
    assert!(matches!(node.inputs[2], Input::Constant(v) if v.abs() < f32::EPSILON));
    assert!(matches!(node.inputs[3], Input::Node { .. }));
    assert!(matches!(node.inputs[4], Input::Node { .. }));

    let raw = parse_ugens(&encode_synthdef(&ir).expect("encode succeeds"))
        .into_iter()
        .find(|ugen| ugen.name == "BufWr")
        .expect("encoded BufWr missing");

    assert_eq!(raw.rate, Rate::Audio.as_byte());
    assert_eq!(raw.inputs.len(), 5);
    assert_eq!(raw.num_outputs, 0);
    assert!(raw.output_rates.is_empty());
}
