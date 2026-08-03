use crate::backend::{InferenceBackend, SegmentationMask, Session, Tensor, TensorType, Error};

pub struct SelfieSegmenterBuilder;

#[derive(Clone, Debug, Default)]
pub struct SelfieSegmenterOptions {
    pub output_type: SegmentationOutputType,
}

#[derive(Clone, Debug, Default)]
pub enum SegmentationOutputType {
    #[default]
    CategoryMask,
    ConfidenceMask,
}

impl SelfieSegmenterBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn output_type(self, _output_type: SegmentationOutputType) -> Self {
        Self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<SelfieSegmenter, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<SelfieSegmenter, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(SelfieSegmenter {
            model,
            session,
        })
    }
}

pub struct SelfieSegmenter {
    model: crate::backend::Model,
    session: Session,
}

impl SelfieSegmenter {
    pub fn segment(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<SegmentationMask, Error> {
        let input_shape = &self.model.inputs[0].shape.clone();
        let input_type = self.model.inputs[0].tensor_type;

        let input_data = match input_type {
            TensorType::F32 => {
                let f32_data: Vec<f32> = image_data.iter()
                    .map(|&p| p as f32 / 255.0)
                    .collect();
                let bytes: Vec<u8> = f32_data.iter()
                    .flat_map(|&f| f.to_le_bytes())
                    .collect();
                bytes
            }
            _ => image_data.to_vec(),
        };

        let input_tensor = Tensor::new(input_type, input_shape.clone(), input_data);
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let output_shape = self.model.outputs[0].shape.clone();

        let mut mask_tensor = Tensor::empty(TensorType::F32, output_shape.clone());
        self.session.get_output(0, &mut mask_tensor)?;

        let (mask_width, mask_height, _num_classes) = if output_shape.len() >= 3 {
            (output_shape[1] as u32, output_shape[2] as u32, output_shape[3] as usize)
        } else {
            (width, height, 0)
        };

        let data = mask_tensor.as_f32();
        let pixels = (mask_width * mask_height) as usize;

        let mut confidence_mask = Vec::with_capacity(pixels);
        let person_class = 15;

        for i in 0..pixels {
            let idx = i + person_class * pixels;
            let confidence = if idx < data.len() {
                (data[idx] + 10.0) / 20.0
            } else {
                0.0
            };
            confidence_mask.push(confidence.max(0.0).min(1.0));
        }

        Ok(SegmentationMask {
            width: mask_width,
            height: mask_height,
            category_mask: Vec::new(),
            confidence_mask: Some(confidence_mask),
        })
    }
}
