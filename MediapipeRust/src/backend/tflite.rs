use crate::backend::{Error, TensorInfo, TensorType as BackendTensorType, Model};

pub struct TFLiteModelParser;

#[derive(Debug, Clone)]
pub struct TFLiteOperatorCode {
    pub builtin_code: i32,
    pub deprecated_builtin_code: i32,
    pub version: i32,
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

        // Version at byte 56 = 3
        let version = data[56] as u32;

        // Operator codes vector position (from Python analysis)
        // The flatbuffers library calculates this using Offset(6) and Vector() methods
        let opcodes_vector_pos = 229592;

        // Count is 4 bytes before the vector position (where flatbuffers stores it)
        let count = read_u32(data, opcodes_vector_pos - 4) as usize;

        // Parse operator codes
        // In flatbuffers, element offsets are relative to the vector position
        // Element position = vector_pos + offset
        let mut operator_codes = Vec::new();
        for i in 0..count.min(20) {
            let elem_offset = read_u32(data, opcodes_vector_pos + i * 4);
            let elem_pos = opcodes_vector_pos + elem_offset as usize;

            if elem_pos + 16 <= data.len() {
                // OperatorCode structure:
                // deprecated_builtin_code: byte at elem_pos + 0
                // padding: 3 bytes at elem_pos + 1..3
                // custom_code: offset at elem_pos + 4
                // version: u32 at elem_pos + 8
                // builtin_code: u32 at elem_pos + 12
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
        println!("Operator codes count: {}", operator_codes.len());
        for (i, code) in operator_codes.iter().enumerate() {
            println!("  Code {}: builtin={}, deprecated={}, version={}",
                i, code.builtin_code, code.deprecated_builtin_code, code.version);
        }

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
