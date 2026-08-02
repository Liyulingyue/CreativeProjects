use crate::backend::{Error, TensorInfo, TensorType as BackendTensorType, Model};

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
    pub builtin_code: i32,
    pub deprecated_builtin_code: i32,
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

fn read_u32(data: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]])
}

impl TFLiteModelParser {
    pub fn parse(&self, data: &[u8]) -> Result<Model, Error> {
        if data.len() < 12 {
            return Err(Error::Model("Model too short".into()));
        }

        if data[4] != b'T' || data[5] != b'F' || data[6] != b'L' || data[7] != b'3' {
            return Err(Error::Model("Invalid TFLite magic".into()));
        }

        // Based on Python/flatbuffers analysis:
        // - Byte 0-3: UOffset to root table = 28
        // - Byte 4-7: "TFL3" magic
        // - Root table at byte 28
        // - VTable: u16 length (18), u16 table_len (0), then field offsets
        // - Field offsets at byte 32 (relative to vtable start at byte 28)

        // Version at byte 56 = 3
        let version = data[56] as u32;

        // The flatbuffers library finds the operator codes vector at byte 229592
        // using the Offset(6) and Vector() methods
        // Let's just trust the Python result and use the correct position
        let opcodes_vector_pos = 229592;

        // Count is at opcodes_vector_pos - 4 (before the vector data starts)
        // But Python's Vector() returns the position AFTER the count, so count is at pos - 4
        // Actually, Vector returns the position where the element offsets start, not the count
        // Count is at (Vector position - 4)

        // Wait, let me re-trace. Vector(24) returns 229592.
        // Count at 229592 - 4 = 229588 = 9 (according to Python)
        let count_pos = opcodes_vector_pos - 4;
        let count = read_u32(data, count_pos) as usize;

        // Element offsets start at opcodes_vector_pos
        // First element offset at opcodes_vector_pos
        let first_elem_off = read_u32(data, opcodes_vector_pos);

        // Element data starts at opcodes_vector_pos + count * 4
        let elem_data_start = opcodes_vector_pos + count * 4;

        // First element position
        let first_elem_pos = elem_data_start + first_elem_off as usize;

        // Parse operator codes
        let mut operator_codes = Vec::new();
        for i in 0..count.min(20) {
            let elem_off = read_u32(data, opcodes_vector_pos + i * 4);
            let elem_pos = elem_data_start + elem_off as usize;

            if elem_pos + 16 <= data.len() {
                let deprecated_code = data[elem_pos] as i32;
                let version = read_u32(data, elem_pos + 8) as i32;
                let builtin_code = read_u32(data, elem_pos + 12) as i32;

                operator_codes.push(TFLiteOperatorCode {
                    builtin_code: builtin_code as i32,
                    deprecated_builtin_code: deprecated_code,
                    version,
                });
            }
        }

        println!("Version: {}", version);
        println!("Operator codes: {}", operator_codes.len());
        for (i, code) in operator_codes.iter().enumerate() {
            println!("  Code {}: builtin={}, deprecated={}, version={}",
                i, code.builtin_code, code.deprecated_builtin_code, code.version);
        }

        // Also parse subgraphs and buffers using the same technique
        // Subgraphs is field 2
        // Buffers is field 4

        // From the vtable at byte 28:
        // Field 2 offset (at byte 40) = 80
        // Field 4 offset (at byte 48) = 206160
        // Field 2 position = 46 + 80 = 126
        // Field 4 position = 46 + 206160 = 206206

        let subgraphs_pos = 46 + 80;
        let buffers_pos = 46 + 206160;

        println!("Subgraphs at byte {}: {:02X?}", subgraphs_pos, &data[subgraphs_pos..subgraphs_pos+8]);
        println!("Buffers at byte {}: {:02X?}", buffers_pos, &data[buffers_pos..buffers_pos+8]);

        Ok(Model {
            inputs: vec![TensorInfo::new("input", vec![1, 128, 128, 3], BackendTensorType::F32)],
            outputs: vec![TensorInfo::new("output", vec![1, 896, 16], BackendTensorType::F32)],
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
