use crate::backend::{Error, TensorInfo, TensorType};

pub struct TFLiteModelParser;

impl TFLiteModelParser {
    pub fn parse(data: &[u8]) -> Result<crate::backend::Model, Error> {
        if data.len() < 16 {
            return Err(Error::Model("Model too short".into()));
        }

        let size_prefix = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let magic = &data[4..8];
        if magic != b"TFL3" {
            return Err(Error::Model(format!("Invalid TFLite magic: {:?}", magic)));
        }

        println!("TFLite header valid: size_prefix={}, magic=TFL3", size_prefix);

        Ok(crate::backend::Model {
            inputs: vec![TensorInfo::new("input", vec![1, 224, 224, 3], TensorType::F32)],
            outputs: vec![TensorInfo::new("output", vec![1, 1000], TensorType::F32)],
        })
    }
}
