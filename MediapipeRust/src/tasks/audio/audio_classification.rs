use crate::backend::{Class, InferenceBackend, Model, Session, Tensor, TensorType, Error};
use std::sync::Arc;

pub struct AudioClassifierBuilder<B: InferenceBackend> {
    backend: B,
    options: AudioClassifierOptions,
}

#[derive(Clone, Debug, Default)]
pub struct AudioClassifierOptions {
    pub sample_rate: u32,
    pub num_channels: u32,
    pub label_allow_list: Vec<String>,
    pub label_deny_list: Vec<String>,
    pub max_results: i32,
}

impl<B: InferenceBackend> AudioClassifierBuilder<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            options: AudioClassifierOptions::default(),
        }
    }

    pub fn sample_rate(mut self, rate: u32) -> Self {
        self.options.sample_rate = rate;
        self
    }

    pub fn num_channels(mut self, channels: u32) -> Self {
        self.options.num_channels = channels;
        self
    }

    pub fn max_results(mut self, max: i32) -> Self {
        self.options.max_results = max;
        self
    }

    pub fn build_from_file(self, path: &str) -> Result<AudioClassifier<B>, Error> {
        let data = std::fs::read(path)?;
        self.build_from_buffer(data)
    }

    pub fn build_from_buffer(self, buffer: Vec<u8>) -> Result<AudioClassifier<B>, Error> {
        let model = self.backend.load_model(&buffer)?;
        Ok(AudioClassifier {
            backend: self.backend,
            model: Arc::new(model),
            options: self.options,
        })
    }
}

pub struct AudioClassifier<B: InferenceBackend> {
    backend: B,
    model: Arc<Model>,
    options: AudioClassifierOptions,
}

impl<B: InferenceBackend> AudioClassifier<B> {
    pub fn new_session(&self) -> Result<AudioClassifierSession<'_, B>, Error> {
        let session = self.backend.create_session(&*self.model)?;
        Ok(AudioClassifierSession {
            classifier: self,
            session,
        })
    }

    pub fn classify(&self, audio_data: &[u8]) -> Result<Vec<Class>, Error> {
        let mut session = self.new_session()?;
        session.classify(audio_data)
    }
}

pub struct AudioClassifierSession<'a, B: InferenceBackend> {
    classifier: &'a AudioClassifier<B>,
    session: Session,
}

impl<'a, B: InferenceBackend> AudioClassifierSession<'a, B> {
    pub fn classify(&mut self, audio_data: &[u8]) -> Result<Vec<Class>, Error> {
        let input_tensor = Tensor::new(
            self.classifier.model.inputs[0].tensor_type,
            self.classifier.model.inputs[0].shape.clone(),
            audio_data.to_vec(),
        );
        self.session.set_input(0, &input_tensor)?;
        self.session.compute()?;

        let mut output_tensor = Tensor::empty(TensorType::F32, self.classifier.model.outputs[0].shape.clone());
        self.session.get_output(0, &mut output_tensor)?;

        let scores = output_tensor.as_f32();
        let mut classes: Vec<Class> = scores
            .iter()
            .enumerate()
            .map(|(i, &s)| Class::new(i as i32, s, format!("class_{}", i)))
            .collect();

        classes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        classes.truncate(self.classifier.options.max_results as usize);

        Ok(classes)
    }
}
