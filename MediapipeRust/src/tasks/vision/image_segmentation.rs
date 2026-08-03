use crate::backend::{InferenceBackend, Model, SegmentationMask, Session, Tensor, TensorType, Error};

#[derive(Clone, Debug, Default)]
pub struct ImageSegmenterOptions {
    pub output_type: ImageSegmenterOutputType,
    pub smooth: bool,
}

#[derive(Clone, Debug, Default)]
pub enum ImageSegmenterOutputType {
    #[default]
    CategoryMask,
    ConfidenceMask,
}

pub struct ImageSegmenterBuilder {
    options: ImageSegmenterOptions,
}

impl ImageSegmenterBuilder {
    pub fn new() -> Self {
        Self {
            options: ImageSegmenterOptions::default(),
        }
    }

    pub fn output_type(mut self, output_type: ImageSegmenterOutputType) -> Self {
        self.options.output_type = output_type;
        self
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.options.smooth = smooth;
        self
    }

    pub fn build_from_file<B: InferenceBackend>(self, backend: &B, path: &str) -> Result<ImageSegmenter, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(backend, data)
    }

    pub fn build_from_buffer<B: InferenceBackend>(self, backend: &B, buffer: Vec<u8>) -> Result<ImageSegmenter, Error> {
        let (model, session) = backend.load_model_and_session(&buffer)?;
        Ok(ImageSegmenter {
            model,
            session,
            options: self.options,
        })
    }
}

pub struct ImageSegmenter {
    model: Model,
    session: Session,
    #[allow(dead_code)]
    options: ImageSegmenterOptions,
}

impl ImageSegmenter {
    pub fn segment(&mut self, image_data: &[u8], _width: u32, _height: u32) -> Result<Vec<SegmentationMask>, Error> {
        let input_tensor = Tensor::new(
            self.model.inputs[0].tensor_type,
            self.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let output_type = self.model.outputs[0].tensor_type;
        let output_shape = self.model.outputs[0].shape.clone();

        let mut mask_tensor = Tensor::empty(output_type, output_shape.clone());
        self.session.get_output(0, &mut mask_tensor)?;

        let (mask_width, mask_height, num_classes) = if output_shape.len() >= 3 {
            (output_shape[1] as u32, output_shape[2] as u32, output_shape[3] as usize)
        } else {
            (256, 256, 0)
        };

        let category_mask: Vec<u8> = if output_type == TensorType::U8 {
            mask_tensor.data.clone()
        } else {
            let data = mask_tensor.as_f32();
            let pixels = (mask_width * mask_height) as usize;
            let mut mask = Vec::with_capacity(pixels);

            for i in 0..pixels {
                let mut max_val = f32::MIN;
                let mut max_class = 0;
                for c in 0..num_classes {
                    let idx = i + c * pixels;
                    if idx < data.len() && data[idx] > max_val {
                        max_val = data[idx];
                        max_class = c;
                    }
                }
                mask.push(max_class as u8);
            }
            mask
        };

        let mask = SegmentationMask {
            width: mask_width,
            height: mask_height,
            category_mask,
            confidence_mask: None,
        };

        Ok(vec![mask])
    }
}
