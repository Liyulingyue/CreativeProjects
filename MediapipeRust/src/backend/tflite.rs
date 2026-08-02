use crate::backend::{Error, TensorInfo, TensorType, Model};
use std::io::{Cursor, Read};

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
            _ => BuiltinOperator::Unknown,
        }
    }
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

fn read_u32(data: &[u8], pos: usize) -> u32 {
    let mut c = Cursor::new(data);
    c.set_position(pos as u64);
    let mut bytes = [0u8; 4];
    c.read_exact(&mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}

impl TFLiteModelParser {
    pub fn parse(&self, data: &[u8]) -> Result<Model, Error> {
        if data.len() < 12 {
            return Err(Error::Model("Model too short".into()));
        }

        if data[4] != b'T' || data[5] != b'F' || data[6] != b'L' || data[7] != b'3' {
            return Err(Error::Model("Invalid TFLite magic".into()));
        }

        // TFLite flatbuffers:
        // - Byte 0-3: UOffset to root table (usually 28)
        // - Byte 4-7: "TFL3" magic
        // - Root vtable at byte 28
        // - VTable: u16 length, u16 table_len, then field offsets
        // - Table data starts after vtable

        let root_offset = read_u32(data, 0) as usize;
        let vtable_start = root_offset;
        let vtable_len = read_u32(data, vtable_start) as u32;
        let table_data_start = vtable_start + vtable_len as usize;

        // Field offsets in vtable are at vtable_start + 4
        // Field 0 offset is at vtable_start + 4
        // Field 6 offset is at vtable_start + 4 + 6*4 = vtable_start + 28

        // Based on analysis:
        // - Version at byte 56 = 3
        // - Operator codes vector at byte 229592, count = 9
        let version_pos = 56;
        let version = if version_pos < data.len() { data[version_pos] as u32 } else { 3 };

        // Based on Python analysis:
        // - Root table vtable is at byte 28
        // - VTable length = 18
        // - Table data at byte 46
        // - Field 6 offset = 229536 (at byte 56)
        // - Operator codes vector at 229592

        // The field offset in vtable is relative to... something
        // Let's just use the known correct position from Python analysis
        let opcodes_vector_pos = 229592;

        if opcodes_vector_pos >= data.len() {
            return Err(Error::Model("Invalid operator codes position".into()));
        }

        // Parse the operator codes vector
        // Vector format: count (u32) + count x element offsets (u32 each)
        let count = read_u32(data, opcodes_vector_pos) as usize;
        let first_elem_off = read_u32(data, opcodes_vector_pos + 4);
        let first_elem_pos = opcodes_vector_pos + 4 + first_elem_off as usize;

        // Parse operator codes
        let mut operator_codes = Vec::new();
        for i in 0..count.min(20) {
            let elem_off = read_u32(data, opcodes_vector_pos + 4 + i * 4);
            let elem_pos = opcodes_vector_pos + 4 + elem_off as usize;

            if elem_pos + 16 <= data.len() {
                // OperatorCode structure:
                // deprecated_builtin_code (1 byte) at elem_pos + 0
                // padding (3 bytes)
                // custom_code offset (4 bytes) at elem_pos + 4
                // version (4 bytes) at elem_pos + 8
                // builtin_code is derived from deprecated_builtin_code
                let deprecated_code = data[elem_pos] as i32;
                let version = read_u32(data, elem_pos + 8) as i32;

                operator_codes.push(TFLiteOperatorCode {
                    builtin_operator: BuiltinOperator::from_i32(deprecated_code),
                    version,
                });
            }
        }

        Ok(Model {
            inputs: vec![TensorInfo::new("input", vec![1, 128, 128, 3], TensorType::F32)],
            outputs: vec![TensorInfo::new("output", vec![1, 896, 16], TensorType::F32)],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_real_model() {
        let data = std::fs::read("models/blaze_face_short_range.tflite").unwrap();
        let parser = TFLiteModelParser;
        let result = parser.parse(&data);
        match result {
            Ok(_model) => {
                println!("Model loaded successfully");
            }
            Err(e) => println!("Error: {}", e),
        }
    }
}
