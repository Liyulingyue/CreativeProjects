use crate::backend::{Class, InferenceBackend, Session, Tensor, TensorType, Error};
use std::sync::Arc;
use crate::backend::Model as BackendModel;

pub struct ImageClassifierBuilder<B: InferenceBackend> {
    backend: B,
    max_results: i32,
}

impl<B: InferenceBackend> ImageClassifierBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self { backend, max_results: 5 }
    }

    pub fn max_results(mut self, max: i32) -> Self {
        self.max_results = max;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<ImageClassifier<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<ImageClassifier<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(ImageClassifier {
            backend: self.backend,
            model: Arc::new(model),
            max_results: self.max_results,
        })
    }
}

pub struct ImageClassifier<B: InferenceBackend> {
    backend: B,
    model: Arc<BackendModel>,
    max_results: i32,
}

impl<B: InferenceBackend> ImageClassifier<B> {
    pub fn new_session(&self) -> Result<ImageClassifierSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(ImageClassifierSession {
            classifier: self,
            session,
        })
    }

    pub fn classify(&self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Class>, Error> {
        let mut session = self.new_session()?;
        session.classify(image_data, width, height)
    }
}

pub struct ImageClassifierSession<'a, B: InferenceBackend> {
    classifier: &'a ImageClassifier<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> ImageClassifierSession<'a, B> {
    pub fn classify(&mut self, image_data: &[u8], width: u32, height: u32) -> Result<Vec<Class>, Error> {
        let input = Tensor::new(
            TensorType::F32,
            vec![1, height as usize, width as usize, 3],
            image_data.to_vec(),
        );

        self.session.set_input(0, &input)?;
        self.session.compute()?;

        let mut output = Tensor::empty(TensorType::F32, vec![1, 1000]);
        self.session.get_output(0, &mut output)?;

        let scores = output.as_f32();
        let mut categories: Vec<Class> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| Class {
                index: i as i32,
                score: s,
                label: format!("class_{}", i),
            })
            .collect();
        categories.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        categories.truncate(self.classifier.max_results as usize);

        Ok(categories)
    }
}
