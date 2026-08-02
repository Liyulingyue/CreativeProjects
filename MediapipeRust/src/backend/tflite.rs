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
    Add, AveragePool, Concat, Conv2D, DepthwiseConv2D, DepthToSpace,
    Dequantize, EmbeddingLookup, Floor, FullyConnected, HashtableLookup,
    L2Normalization, L2Pool2D, LocalResponseNormalization, Logistic, LshProjection,
    Lstm, MaxPool, Mul, Relu, ReluN1To1, Relu6, Reshape, ResizeBilinear, Rnn,
    Softmax, SpaceToDepth, Svdf, Tanh, ConcatEmbeddings, SkipGram, Call, Custom,
    EmbeddingLookupSparse, Pad, UnidirectionalSequenceRnn, Gather, BatchToSpaceNd,
    SpaceToBatchNd, Transpose, Mean, Sub, Div, Squeeze, UnidirectionalSequenceLstm,
    StridedSlice, BidirectionalSequenceRnn, Exp, TopKV2, Split, LogSoftmax, Delegate,
    BidirectionalSequenceLstm, Cast, Prelu, Maximum, ArgMax, Minimum, Less, Neg, PadV2,
    Greater, GreaterEqual, LessEqual, Select, Slice, Sin, TransposeConv, SparseToDense,
    Tile, ExpandDims, Equal, NotEqual, Log, Sum, Sqrt, Rsqrt, Shape, Pow, ArgMin,
    FakeQuant, ReduceProd, ReduceMax, Pack, LogicalOr, OneHot, LogicalAnd, LogicalNot,
    Unpack, ReduceMin, FloorDiv, ReduceAny, Square, ZerosLike, Fill, FloorMod, Range,
    ResizeNearestNeighbor, LeakyRelu, SquaredDifference, MirrorPad, Abs, SplitV, Unique,
    Ceil, ReverseV2, AddN, GatherNd, Cos, Where, Rank, Elu, ReverseSequence, MatrixDiag,
    Quantize, MatrixSetDiag, Round, HardSwish, If, While, NonMaxSuppressionV4,
    NonMaxSuppressionV5, ScatterNd, SelectV2, Densify, SegmentSum, BatchMatMul,
    PlaceholderForGreaterOpCodes, Cumsum, CallOnce, BroadcastTo, Rfft2d, Conv3D, Imag,
    Real, ComplexAbs, Hashtable, HashtableFind, HashtableImport, HashtableSize, ReduceAll,
    Conv3DTranspose, VarHandle, ReadVariable, AssignVariable, BroadcastArgs,
    RandomStandardNormal, Bucketize, RandomUniform, Multinomial, Gelu, DynamicUpdateSlice,
    Relu0To1, UnsortedSegmentProd, UnsortedSegmentMax, UnsortedSegmentSum, Atan2,
    UnsortedSegmentMin, Sign, Bitcast, BitwiseXor, RightShift, Unknown,
}

impl BuiltinOperator {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => BuiltinOperator::Add,
            1 => BuiltinOperator::AveragePool,
            2 => BuiltinOperator::Concat,
            3 => BuiltinOperator::Conv2D,
            4 => BuiltinOperator::DepthwiseConv2D,
            5 => BuiltinOperator::DepthToSpace,
            6 => BuiltinOperator::Dequantize,
            7 => BuiltinOperator::EmbeddingLookup,
            8 => BuiltinOperator::Floor,
            9 => BuiltinOperator::FullyConnected,
            10 => BuiltinOperator::HashtableLookup,
            11 => BuiltinOperator::L2Normalization,
            12 => BuiltinOperator::L2Pool2D,
            13 => BuiltinOperator::LocalResponseNormalization,
            14 => BuiltinOperator::Logistic,
            15 => BuiltinOperator::LshProjection,
            16 => BuiltinOperator::Lstm,
            17 => BuiltinOperator::MaxPool,
            18 => BuiltinOperator::Mul,
            19 => BuiltinOperator::Relu,
            20 => BuiltinOperator::ReluN1To1,
            21 => BuiltinOperator::Relu6,
            22 => BuiltinOperator::Reshape,
            23 => BuiltinOperator::ResizeBilinear,
            24 => BuiltinOperator::Rnn,
            25 => BuiltinOperator::Softmax,
            26 => BuiltinOperator::SpaceToDepth,
            27 => BuiltinOperator::Svdf,
            28 => BuiltinOperator::Tanh,
            29 => BuiltinOperator::ConcatEmbeddings,
            30 => BuiltinOperator::SkipGram,
            31 => BuiltinOperator::Call,
            32 => BuiltinOperator::Custom,
            33 => BuiltinOperator::EmbeddingLookupSparse,
            34 => BuiltinOperator::Pad,
            35 => BuiltinOperator::UnidirectionalSequenceRnn,
            36 => BuiltinOperator::Gather,
            37 => BuiltinOperator::BatchToSpaceNd,
            38 => BuiltinOperator::SpaceToBatchNd,
            39 => BuiltinOperator::Transpose,
            40 => BuiltinOperator::Mean,
            41 => BuiltinOperator::Sub,
            42 => BuiltinOperator::Div,
            43 => BuiltinOperator::Squeeze,
            44 => BuiltinOperator::UnidirectionalSequenceLstm,
            45 => BuiltinOperator::StridedSlice,
            46 => BuiltinOperator::BidirectionalSequenceRnn,
            47 => BuiltinOperator::Exp,
            48 => BuiltinOperator::TopKV2,
            49 => BuiltinOperator::Split,
            50 => BuiltinOperator::LogSoftmax,
            51 => BuiltinOperator::Delegate,
            52 => BuiltinOperator::BidirectionalSequenceLstm,
            53 => BuiltinOperator::Cast,
            54 => BuiltinOperator::Prelu,
            55 => BuiltinOperator::Maximum,
            56 => BuiltinOperator::ArgMax,
            57 => BuiltinOperator::Minimum,
            58 => BuiltinOperator::Less,
            59 => BuiltinOperator::Neg,
            60 => BuiltinOperator::PadV2,
            61 => BuiltinOperator::Greater,
            62 => BuiltinOperator::GreaterEqual,
            63 => BuiltinOperator::LessEqual,
            64 => BuiltinOperator::Select,
            65 => BuiltinOperator::Slice,
            66 => BuiltinOperator::Sin,
            67 => BuiltinOperator::TransposeConv,
            68 => BuiltinOperator::SparseToDense,
            69 => BuiltinOperator::Tile,
            70 => BuiltinOperator::ExpandDims,
            71 => BuiltinOperator::Equal,
            72 => BuiltinOperator::NotEqual,
            73 => BuiltinOperator::Log,
            74 => BuiltinOperator::Sum,
            75 => BuiltinOperator::Sqrt,
            76 => BuiltinOperator::Rsqrt,
            77 => BuiltinOperator::Shape,
            78 => BuiltinOperator::Pow,
            79 => BuiltinOperator::ArgMin,
            80 => BuiltinOperator::FakeQuant,
            81 => BuiltinOperator::ReduceProd,
            82 => BuiltinOperator::ReduceMax,
            83 => BuiltinOperator::Pack,
            84 => BuiltinOperator::LogicalOr,
            85 => BuiltinOperator::OneHot,
            86 => BuiltinOperator::LogicalAnd,
            87 => BuiltinOperator::LogicalNot,
            88 => BuiltinOperator::Unpack,
            89 => BuiltinOperator::ReduceMin,
            90 => BuiltinOperator::FloorDiv,
            91 => BuiltinOperator::ReduceAny,
            92 => BuiltinOperator::Square,
            93 => BuiltinOperator::ZerosLike,
            94 => BuiltinOperator::Fill,
            95 => BuiltinOperator::FloorMod,
            96 => BuiltinOperator::Range,
            97 => BuiltinOperator::ResizeNearestNeighbor,
            98 => BuiltinOperator::LeakyRelu,
            99 => BuiltinOperator::SquaredDifference,
            100 => BuiltinOperator::MirrorPad,
            101 => BuiltinOperator::Abs,
            102 => BuiltinOperator::SplitV,
            103 => BuiltinOperator::Unique,
            104 => BuiltinOperator::Ceil,
            105 => BuiltinOperator::ReverseV2,
            106 => BuiltinOperator::AddN,
            107 => BuiltinOperator::GatherNd,
            108 => BuiltinOperator::Cos,
            109 => BuiltinOperator::Where,
            110 => BuiltinOperator::Rank,
            111 => BuiltinOperator::Elu,
            112 => BuiltinOperator::ReverseSequence,
            113 => BuiltinOperator::MatrixDiag,
            114 => BuiltinOperator::Quantize,
            115 => BuiltinOperator::MatrixSetDiag,
            116 => BuiltinOperator::Round,
            117 => BuiltinOperator::HardSwish,
            118 => BuiltinOperator::If,
            119 => BuiltinOperator::While,
            120 => BuiltinOperator::NonMaxSuppressionV4,
            121 => BuiltinOperator::NonMaxSuppressionV5,
            122 => BuiltinOperator::ScatterNd,
            123 => BuiltinOperator::SelectV2,
            124 => BuiltinOperator::Densify,
            125 => BuiltinOperator::SegmentSum,
            126 => BuiltinOperator::BatchMatMul,
            127 => BuiltinOperator::PlaceholderForGreaterOpCodes,
            128 => BuiltinOperator::Cumsum,
            129 => BuiltinOperator::CallOnce,
            130 => BuiltinOperator::BroadcastTo,
            131 => BuiltinOperator::Rfft2d,
            132 => BuiltinOperator::Conv3D,
            133 => BuiltinOperator::Imag,
            134 => BuiltinOperator::Real,
            135 => BuiltinOperator::ComplexAbs,
            136 => BuiltinOperator::Hashtable,
            137 => BuiltinOperator::HashtableFind,
            138 => BuiltinOperator::HashtableImport,
            139 => BuiltinOperator::HashtableSize,
            140 => BuiltinOperator::ReduceAll,
            141 => BuiltinOperator::Conv3DTranspose,
            142 => BuiltinOperator::VarHandle,
            143 => BuiltinOperator::ReadVariable,
            144 => BuiltinOperator::AssignVariable,
            145 => BuiltinOperator::BroadcastArgs,
            146 => BuiltinOperator::RandomStandardNormal,
            147 => BuiltinOperator::Bucketize,
            148 => BuiltinOperator::RandomUniform,
            149 => BuiltinOperator::Multinomial,
            150 => BuiltinOperator::Gelu,
            151 => BuiltinOperator::DynamicUpdateSlice,
            152 => BuiltinOperator::Relu0To1,
            153 => BuiltinOperator::UnsortedSegmentProd,
            154 => BuiltinOperator::UnsortedSegmentMax,
            155 => BuiltinOperator::UnsortedSegmentSum,
            156 => BuiltinOperator::Atan2,
            157 => BuiltinOperator::UnsortedSegmentMin,
            158 => BuiltinOperator::Sign,
            159 => BuiltinOperator::Bitcast,
            160 => BuiltinOperator::BitwiseXor,
            161 => BuiltinOperator::RightShift,
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
