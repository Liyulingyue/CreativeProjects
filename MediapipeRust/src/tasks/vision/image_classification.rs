use crate::backend::{Backend, CategoriesFilter, Class, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct ImageClassifierBuilder<B: InferenceBackend> {
    backend: B,
    max_results: i32,
    classifier_options: ImageClassifierOptions,
}

#[derive(Clone, Debug, Default)]
pub struct ImageClassifierOptions {
    pub label_allow_list: Vec<String>,
    pub label_deny_list: Vec<String>,
    pub score_threshold: Option<f32>,
}

impl<B: InferenceBackend> ImageClassifierBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            max_results: 5,
            classifier_options: ImageClassifierOptions::default(),
        }
    }

    pub fn max_results(mut self, max: i32) -> Self {
        self.max_results = max;
        self
    }

    pub fn score_threshold(mut self, threshold: f32) -> Self {
        self.classifier_options.score_threshold = Some(threshold);
        self
    }

    pub fn label_allow_list(mut self, labels: Vec<String>) -> Self {
        self.classifier_options.label_allow_list = labels;
        self
    }

    pub fn label_deny_list(mut self, labels: Vec<String>) -> Self {
        self.classifier_options.label_deny_list = labels;
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
            options: self.classifier_options,
        })
    }
}

pub struct ImageClassifier<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    max_results: i32,
    options: ImageClassifierOptions,
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
        let input_tensor = self.prepare_input(image_data, width, height)?;
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let output_shape = &self.classifier.model.outputs[0].shape;
        let output_type = self.classifier.model.outputs[0].tensor_type;
        let mut output = Tensor::empty(output_type, output_shape.clone());
        self.session.get_output(0, &mut output)?;

        let mut classes = self.postprocess_output(&output)?;
        let filter = CategoriesFilter {
            label_allow_list: self.classifier.options.label_allow_list.clone(),
            label_deny_list: self.classifier.options.label_deny_list.clone(),
            min_score: self.classifier.options.score_threshold.unwrap_or(f32::MIN),
        };
        filter.filter(&mut classes);
        classes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        classes.truncate(self.classifier.max_results as usize);

        Ok(classes)
    }

    fn prepare_input(&self, image_data: &[u8], width: u32, height: u32) -> Result<Tensor, Error> {
        let input_shape = &self.classifier.model.inputs[0].shape;
        let input_type = self.classifier.model.inputs[0].tensor_type;

        let tensor = Tensor::new(input_type, input_shape.clone(), image_data.to_vec());
        Ok(tensor)
    }

    fn postprocess_output(&self, output: &Tensor) -> Result<Vec<Class>, Error> {
        let scores = output.as_f32();
        let mut classes: Vec<Class> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| Class::new(i as i32, s, format!("class_{}", i)))
            .collect();

        classes.retain(|c| c.score > 0.0);
        Ok(classes)
    }
}
