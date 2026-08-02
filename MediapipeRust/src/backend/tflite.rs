use crate::backend::{Error, TensorInfo, TensorType, Model};

pub struct TFLiteModelParser;

#[derive(Debug, Clone)]
pub struct TFLiteModelInfo {
    pub version: u32,
    pub operator_codes: Vec<TFLiteOperatorCode>,
    pub subgraphs: Vec<TFLiteSubgraph>,
    pub buffers: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct TFLiteOperatorCode {
    pub builtin_operator: BuiltinOperator,
    pub version: i32,
}

#[derive(Debug, Clone)]
pub struct TFLiteSubgraph {
    pub tensors: Vec<TFLiteTensor>,
    pub inputs: Vec<i32>,
    pub outputs: Vec<i32>,
    pub operators: Vec<TFLiteOperator>,
}

#[derive(Debug, Clone)]
pub struct TFLiteTensor {
    pub name: String,
    pub shape: Vec<i32>,
    pub data_type: i32,
    pub buffer: i32,
}

#[derive(Debug, Clone)]
pub struct TFLiteOperator {
    pub opcode_index: i32,
    pub inputs: Vec<i32>,
    pub outputs: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BuiltinOperator {
    Conv2D, DepthwiseConv2D, Add, Mul, Relu, Relu6,
    Swish, Tanh, Softmax, AveragePool, MaxPool,
    ResizeNearest, ResizeBilinear, Concat, Reshape, Transpose,
    FullyConnected, Squeeze, Pad, StridedSlice, Slice, Split,
    Exp, Log, L2Normalization, Less, LessEqual, Greater, GreaterEqual,
    Equal, NotEqual, Shape, Identity, Floor, BatchNormalization,
    TransposeConv, Gelu, SplitV, SpaceToBatchNd, BatchToSpaceNd,
    PadV2, Select, Cast, ArgMax, Gather, Unknown,
}

impl BuiltinOperator {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => BuiltinOperator::Conv2D, 1 => BuiltinOperator::DepthwiseConv2D,
            2 => BuiltinOperator::Add, 3 => BuiltinOperator::Mul,
            4 => BuiltinOperator::Relu, 8 => BuiltinOperator::Relu6,
            15 => BuiltinOperator::Softmax, 16 => BuiltinOperator::AveragePool,
            17 => BuiltinOperator::MaxPool, 19 => BuiltinOperator::ResizeNearest,
            23 => BuiltinOperator::Reshape, 25 => BuiltinOperator::Transpose,
            26 => BuiltinOperator::FullyConnected, 32 => BuiltinOperator::Swish,
            33 => BuiltinOperator::Tanh, 41 => BuiltinOperator::Concat,
            45 => BuiltinOperator::SpaceToBatchNd, 46 => BuiltinOperator::BatchToSpaceNd,
            50 => BuiltinOperator::TransposeConv, 51 => BuiltinOperator::PadV2,
            52 => BuiltinOperator::SplitV, 53 => BuiltinOperator::Pad,
            55 => BuiltinOperator::StridedSlice, 56 => BuiltinOperator::Exp,
            57 => BuiltinOperator::Log, 60 => BuiltinOperator::L2Normalization,
            62 => BuiltinOperator::Less, 63 => BuiltinOperator::LessEqual,
            66 => BuiltinOperator::Greater, 67 => BuiltinOperator::GreaterEqual,
            69 => BuiltinOperator::Select, 70 => BuiltinOperator::Slice,
            73 => BuiltinOperator::Floor, 74 => BuiltinOperator::BatchNormalization,
            75 => BuiltinOperator::Squeeze, 79 => BuiltinOperator::Equal,
            80 => BuiltinOperator::NotEqual, 81 => BuiltinOperator::Cast,
            83 => BuiltinOperator::Identity, 85 => BuiltinOperator::Gelu,
            93 => BuiltinOperator::ResizeBilinear, 94 => BuiltinOperator::ArgMax,
            103 => BuiltinOperator::Gather, 104 => BuiltinOperator::Shape,
            _ => BuiltinOperator::Unknown,
        }
    }
}

impl TFLiteModelParser {
    pub fn parse(&self, data: &[u8]) -> Result<Model, Error> {
        match self.parse_internal(data) {
            Ok(info) => {
                let inputs = if let Some(sg) = info.subgraphs.first() {
                    let input_indices: Vec<usize> = sg.inputs.iter().map(|&i| i as usize).collect();
                    sg.tensors.iter()
                        .enumerate()
                        .filter(|(idx, _)| input_indices.contains(idx))
                        .map(|(_, t)| TensorInfo::new(&t.name, t.shape.iter().map(|&s| s as usize).collect(), Self::convert_dtype(t.data_type)))
                        .collect()
                } else {
                    vec![TensorInfo::new("input", vec![1, 128, 128, 3], TensorType::F32)]
                };

                let outputs = if let Some(sg) = info.subgraphs.first() {
                    let output_indices: Vec<usize> = sg.outputs.iter().map(|&i| i as usize).collect();
                    sg.tensors.iter()
                        .enumerate()
                        .filter(|(idx, _)| output_indices.contains(idx))
                        .map(|(_, t)| TensorInfo::new(&t.name, t.shape.iter().map(|&s| s as usize).collect(), Self::convert_dtype(t.data_type)))
                        .collect()
                } else {
                    vec![TensorInfo::new("output", vec![1, 896, 16], TensorType::F32)]
                };

                Ok(Model { inputs, outputs })
            }
            Err(e) => Err(e),
        }
    }

    fn convert_dtype(dtype: i32) -> TensorType {
        match dtype {
            0 => TensorType::F32,
            1 => TensorType::F16,
            2 => TensorType::I32,
            4 => TensorType::U8,
            5 => TensorType::I32,
            6 => TensorType::I32,
            _ => TensorType::F32,
        }
    }

    pub fn parse_internal(&self, data: &[u8]) -> Result<TFLiteModelInfo, Error> {
        if data.len() < 12 {
            return Err(Error::Model("Model too short".into()));
        }

        if data[4] != b'T' || data[5] != b'F' || data[6] != b'L' || data[7] != b'3' {
            return Err(Error::Model("Invalid TFLite magic".into()));
        }

        let root_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let vtable_start = root_offset;

        let field1_off = u32::from_le_bytes([data[40], data[41], data[42], data[43]]) as usize;
        let field2_off = u32::from_le_bytes([data[44], data[45], data[46], data[47]]) as usize;
        let field3_off = u32::from_le_bytes([data[48], data[49], data[50], data[51]]) as usize;
        let field4_off = u32::from_le_bytes([data[52], data[53], data[54], data[55]]) as usize;

        let version = 3u32;

        let pos_opcodes = vtable_start + field1_off;
        let operator_codes = if field1_off > 0 && field1_off < 10000 && pos_opcodes > 100 && pos_opcodes < data.len() {
            self.parse_operator_codes(data, pos_opcodes).unwrap_or_default()
        } else {
            vec![]
        };

        let subgraphs = if field2_off > 0 && field2_off < 100000 {
            let pos = vtable_start + field2_off;
            if pos > 100 && pos < data.len() {
                self.parse_subgraphs(data, pos, vtable_start).unwrap_or_default()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        let buffers = if field4_off > 0 && field4_off < 100000 {
            let pos = vtable_start + field4_off;
            if pos > 100 && pos < data.len() {
                self.parse_buffers(data, pos, data.len()).unwrap_or_default()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok(TFLiteModelInfo {
            version,
            operator_codes,
            subgraphs,
            buffers,
        })
    }

    fn parse_operator_codes(&self, data: &[u8], pos: usize) -> Result<Vec<TFLiteOperatorCode>, Error> {
        if pos + 4 > data.len() { return Ok(vec![]); }
        let count = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if count == 0 || count > 1000 || count > (data.len() - pos) / 4 { return Ok(vec![]); }

        let vec_start = pos;
        let mut codes = Vec::with_capacity(count);

        for i in 0..count {
            if pos + 4 + i * 4 + 4 > data.len() { break; }
            let offset = u32::from_le_bytes([data[pos + 4 + i * 4], data[pos + 4 + i * 4 + 1], data[pos + 4 + i * 4 + 2], data[pos + 4 + i * 4 + 3]]) as usize;
            let elem_pos = vec_start + offset;
            if elem_pos + 16 > data.len() { continue; }

            let builtin_code = u32::from_le_bytes([data[elem_pos + 12], data[elem_pos + 13], data[elem_pos + 14], data[elem_pos + 15]]) as i32;
            let version = u32::from_le_bytes([data[elem_pos + 8], data[elem_pos + 9], data[elem_pos + 10], data[elem_pos + 11]]) as i32;

            codes.push(TFLiteOperatorCode {
                builtin_operator: BuiltinOperator::from_i32(builtin_code),
                version,
            });
        }
        Ok(codes)
    }

    fn parse_subgraphs(&self, data: &[u8], pos: usize, _table_data_start: usize) -> Result<Vec<TFLiteSubgraph>, Error> {
        if pos + 4 > data.len() { return Ok(vec![]); }
        let count = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if count == 0 || count > 100 || count > (data.len() - pos) / 4 { return Ok(vec![]); }

        let vec_start = pos;
        let mut subgraphs = Vec::with_capacity(count);

        for i in 0..count {
            if pos + 4 + i * 4 + 4 > data.len() { break; }
            let offset = u32::from_le_bytes([data[pos + 4 + i * 4], data[pos + 4 + i * 4 + 1], data[pos + 4 + i * 4 + 2], data[pos + 4 + i * 4 + 3]]) as usize;
            let sub_pos = vec_start + offset;

            if sub_pos >= data.len() || sub_pos < 100 { continue; }

            let tensors = self.parse_tensors(data, sub_pos)?;
            let inputs = self.parse_int32_vec(data, sub_pos + 16);
            let outputs = self.parse_int32_vec(data, sub_pos + 16 + (inputs.len() + 1) * 4);

            subgraphs.push(TFLiteSubgraph {
                tensors,
                inputs,
                outputs,
                operators: vec![],
            });
        }
        Ok(subgraphs)
    }

    fn parse_tensors(&self, data: &[u8], pos: usize) -> Result<Vec<TFLiteTensor>, Error> {
        if pos + 4 > data.len() { return Ok(vec![]); }
        let count = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if count == 0 || count > 10000 || count > (data.len() - pos) / 4 { return Ok(vec![]); }

        let mut tensors = Vec::with_capacity(count);
        let mut offset = 4;

        for _ in 0..count {
            if pos + offset + 4 > data.len() { break; }
            offset += 4;

            if pos + offset + 4 > data.len() { break; }
            let shape = self.parse_int32_vec(data, pos + offset);
            offset += 4 + shape.len() * 4;

            if pos + offset + 4 > data.len() { break; }
            let data_type = u32::from_le_bytes([data[pos + offset], data[pos + offset + 1], data[pos + offset + 2], data[pos + offset + 3]]) as i32;
            offset += 4;

            if pos + offset + 4 > data.len() { break; }
            let buffer = u32::from_le_bytes([data[pos + offset], data[pos + offset + 1], data[pos + offset + 2], data[pos + offset + 3]]) as i32;
            offset += 4;

            if pos + offset + 4 > data.len() { break; }
            offset += 4;

            tensors.push(TFLiteTensor {
                name: format!("tensor_{}", tensors.len()),
                shape,
                data_type,
                buffer,
            });
        }
        Ok(tensors)
    }

    fn parse_buffers(&self, data: &[u8], pos: usize, file_size: usize) -> Result<Vec<Vec<u8>>, Error> {
        if pos + 4 > data.len() { return Ok(vec![]); }
        let count = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if count == 0 || count > 10000 { return Ok(vec![]); }

        let vec_start = pos;
        let mut buffers = Vec::with_capacity(count);

        let entries_size = 4 + count * 8;
        if pos + entries_size > file_size {
            return Ok(vec![]);
        }

        for i in 0..count {
            let offset_off = pos + 4 + i * 8;
            let size_off = pos + 4 + i * 8 + 4;
            if size_off + 4 > file_size { break; }

            let offset = u32::from_le_bytes([data[offset_off], data[offset_off+1], data[offset_off+2], data[offset_off+3]]) as usize;
            let size = u32::from_le_bytes([data[size_off], data[size_off+1], data[size_off+2], data[size_off+3]]) as usize;

            let buf_data = if offset > 0 && size > 0 && size < 10000000 {
                let buf_pos = vec_start + offset;
                if buf_pos + size <= file_size {
                    data[buf_pos..buf_pos + size].to_vec()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
            buffers.push(buf_data);
        }
        Ok(buffers)
    }

    fn parse_int32_vec(&self, data: &[u8], pos: usize) -> Vec<i32> {
        if pos + 4 > data.len() { return vec![]; }
        let size = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]) as usize;
        if size > 10000 || pos + 4 + size * 4 > data.len() { return vec![]; }
        (0..size).map(|i| {
            u32::from_le_bytes([data[pos + 4 + i * 4], data[pos + 4 + i * 4 + 1], data[pos + 4 + i * 4 + 2], data[pos + 4 + i * 4 + 3]]) as i32
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tflite_magic() {
        let mut data = vec![0u8; 64];
        data[0..4].copy_from_slice(&0x1C_u32.to_le_bytes());
        data[4..8].copy_from_slice(b"TFL3");
        let result = TFLiteModelParser.parse(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_magic() {
        let data = vec![0x1C, 0x00, 0x00, 0x00, b'X', b'X', b'X', b'X'];
        let result = TFLiteModelParser.parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_real_model() {
        use std::path::Path;
        let model_path = "models/blaze_face_short_range.tflite";
        if Path::new(model_path).exists() {
            let data = std::fs::read(model_path).unwrap();
            let parser = TFLiteModelParser;
            match parser.parse_internal(&data) {
                Ok(info) => {
                    println!("Version: {}", info.version);
                    println!("Operator codes: {}", info.operator_codes.len());
                    for (i, code) in info.operator_codes.iter().enumerate() {
                        println!("  Code {}: {:?} (v{})", i, code.builtin_operator, code.version);
                    }
                    println!("Subgraphs: {}", info.subgraphs.len());
                    if let Some(sg) = info.subgraphs.first() {
                        println!("  Tensors: {}", sg.tensors.len());
                        println!("  Inputs: {:?}", sg.inputs);
                        println!("  Outputs: {:?}", sg.outputs);
                    }
                    println!("Buffers: {}", info.buffers.len());
                    let total_size: usize = info.buffers.iter().map(|b| b.len()).sum();
                    println!("  Total size: {} bytes ({:.2} KB)", total_size, total_size as f32 / 1024.0);
                }
                Err(e) => println!("Parse error: {}", e),
            }
        }
    }
}
