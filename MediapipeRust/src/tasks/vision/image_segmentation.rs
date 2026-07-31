use crate::backend::{InferenceBackend, Model, SegmentationMask, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct ImageSegmenterBuilder<B: InferenceBackend> {
    backend: B,
    options: ImageSegmenterOptions,
}

#[derive(Clone, Debug, Default)]
pub struct ImageSegmenterOptions {
    pub output_type: SegmentationOutputType,
    pub smooth: bool,
}

#[derive(Clone, Debug, Default)]
pub enum SegmentationOutputType {
    #[default]
    CategoryMask,
    ConfidenceMask,
}

impl<B: InferenceBackend> ImageSegmenterBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: ImageSegmenterOptions::default(),
        }
    }

    pub fn output_type(mut self, output_type: SegmentationOutputType) -> Self {
        self.options.output_type = output_type;
        self
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        self.options.smooth = smooth;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<ImageSegmenter<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<ImageSegmenter<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(ImageSegmenter {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct ImageSegmenter<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: ImageSegmenterOptions,
}

impl<B: InferenceBackend> ImageSegmenter<B> {
    pub fn new_session(&self) -> Result<ImageSegmenterSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(ImageSegmenterSession {
            segmenter: self,
            session,
        })
    }

    pub fn segment(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<SegmentationMask>, Error> {
        let mut session = self.new_session()?;
        session.segment(image_data, width, height)
    }
}

pub struct ImageSegmenterSession<'a, B: InferenceBackend> {
    segmenter: &'a ImageSegmenter<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> ImageSegmenterSession<'a, B> {
    pub fn segment(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<SegmentationMask>, Error> {
        let input_tensor = Tensor::new(
            self.segmenter.model.inputs[0].tensor_type,
            self.segmenter.model.inputs[0].shape.clone(),
            image_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut mask_tensor = Tensor::empty(TensorType::U8, self.segmenter.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut mask_tensor)?;

        let output_shape = &self.segmenter.model.outputs[0].shape;
        let (mask_width, mask_height) = if output_shape.len() >= 3 {
            (output_shape[1] as u32, output_shape[2] as u32)
        } else {
            (width, height)
        };
        let mask = SegmentationMask {
            width: mask_width,
            height: mask_height,
            category_mask: mask_tensor.data.clone(),
            confidence_mask: None,
        };

        Ok(vec![mask])
    }
}
