use std::path::Path;
use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;
use sherpa_onnx::{
    OfflineRecognizer as NonStreamingAsrRecognizer, 
    OfflineRecognizerConfig as NonStreamingAsrRecognizerConfig,
    OfflineSenseVoiceModelConfig,
};

/// ASR Provider trait，支持远程 HTTP 和本地 Sherpa-Onnx 两种实现
#[async_trait]
pub trait AsrProvider: Send + Sync {
    async fn transcribe(&self, audio_path: &Path, language: &str) -> Result<String, String>;
}

/// 远程 HTTP ASR 实现
pub struct HttpAsrProvider {
    url: String,
}

impl HttpAsrProvider {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
        }
    }
}

#[async_trait]
impl AsrProvider for HttpAsrProvider {
    async fn transcribe(&self, audio_path: &Path, language: &str) -> Result<String, String> {
        let bytes = std::fs::read(audio_path).map_err(|e| format!("读取WAV失败: {}", e))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

        let mut req = client.post(format!("{}/transcribe", self.url));
        req = req.header("Content-Type", "audio/wav");
        if !language.is_empty() && language != "auto" {
            req = req.header("X-Language", language);
        }
        req = req.body(bytes);

        let resp = req.send().await.map_err(|e| format!("请求失败: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("服务器错误 {}: {}", status, body));
        }

        #[derive(serde::Deserialize)]
        struct Resp {
            text: String,
        }

        let result: Resp = resp.json().await.map_err(|e| format!("解析响应正文失败: {}", e))?;
        Ok(result.text)
    }
}

/// 本地 Sherpa-Onnx ASR 实现 (延迟加载，支持 sensevoice-small / sensevoice-small-int8)
pub struct LocalSherpaAsrProvider {
    recognizer: Arc<RwLock<Option<NonStreamingAsrRecognizer>>>,
    model_name: Arc<RwLock<String>>,
}

impl LocalSherpaAsrProvider {
    pub fn new() -> Self {
        Self {
            recognizer: Arc::new(RwLock::new(None)),
            model_name: Arc::new(RwLock::new("sensevoice-small".to_string())),
        }
    }

    /// 切换本地模型（切换时会清空已加载的 recognizer，下次识别时重新加载）
    pub fn set_model(&self, name: &str) {
        let current = self.model_name.read().clone();
        if current != name {
            *self.model_name.write() = name.to_string();
            *self.recognizer.write() = None;
        }
    }

    fn ensure_loaded(&self) -> Result<(), String> {
        let model_name = self.model_name.read().clone();
        
        // 提取基础模型名（用于存放目录名）
        // 如果是 int8 结尾，目录保持为 sensevoice-small，但文件名不同
        let base_dir_name = if model_name.ends_with("-int8") {
            model_name.trim_end_matches("-int8")
        } else {
            &model_name
        };
        
        let model_dir = std::path::Path::new("models").join(base_dir_name);

        let model_file = if model_name.ends_with("-int8") {
            model_dir.join("model.int8.onnx")
        } else {
            model_dir.join("model.onnx")
        };
        let tokens_file = model_dir.join("tokens.txt");

        let mut lock = self.recognizer.write();
        if lock.is_some() {
            return Ok(());
        }

        // 如果模型文件不存在则尝试自动下载（仅支持 float32 模型的自动下载）
        if !model_file.exists() || !tokens_file.exists() {
            if model_name.ends_with("-int8") {
                return Err(format!("本地 int8 模型不存在: {:?}\n请手动下载 model.int8.onnx 并放入目录。", model_file));
            }
            
            println!("模型文件不存在，开始自动下载 SenseVoice-Small...");
            std::fs::create_dir_all(&model_dir).map_err(|e| format!("创建模型目录失败: {}", e))?;

            let base_url = "https://www.modelscope.cn/api/v1/models/pengzhendong/sherpa-onnx-sense-voice-zh-en-ja-ko-yue/repo?Revision=master&FilePath=";
            let files = if model_name.ends_with("-int8") {
                vec![
                    ("model.int8.onnx", model_dir.join("model.int8.onnx")),
                    ("tokens.txt", model_dir.join("tokens.txt")),
                ]
            } else {
                vec![
                    ("model.onnx", model_dir.join("model.onnx")),
                    ("tokens.txt", model_dir.join("tokens.txt")),
                ]
            };

            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(1200))
                .build()
                .map_err(|e| format!("创建下载客户端失败: {}", e))?;

            for (name, dest) in &files {
                if dest.exists() {
                    println!("  {} 已存在，跳过", name);
                    continue;
                }
                println!("  正在从 ModelScope 下载 {} ...", name);
                let url = format!("{}{}", base_url, name);
                let bytes = client.get(&url)
                    .send()
                    .and_then(|r| r.bytes())
                    .map_err(|e| format!("下载 {} 失败: {}", name, e))?;
                std::fs::write(dest, &bytes)
                    .map_err(|e| format!("写入 {} 失败: {}", name, e))?;
                println!("  {} 下载完成 ({:.1} MB)", name, bytes.len() as f64 / 1_048_576.0);
            }
            println!("模型下载完成。");
        }

        println!("正在初始化本地 ASR 模型 ({})...", model_name);

        let mut config = NonStreamingAsrRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(model_file.to_string_lossy().into_owned()),
            language: None,
            use_itn: true,
        };
        config.model_config.tokens = Some(tokens_file.to_string_lossy().into_owned());
        config.model_config.num_threads = 4;

        let recognizer = NonStreamingAsrRecognizer::create(&config)
            .ok_or_else(|| format!("加载模型失败 (模型目录: {:?})", model_dir))?;

        *lock = Some(recognizer);
        Ok(())
    }
}

impl Default for LocalSherpaAsrProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AsrProvider for LocalSherpaAsrProvider {
    /// 使用本地 sherpa-onnx 模型进行语音识别
    async fn transcribe(&self, audio_path: &Path, _language: &str) -> Result<String, String> {
        // 延迟加载
        self.ensure_loaded()?;

        let lock = self.recognizer.read();
        let recognizer = lock.as_ref().unwrap();

        // 读取音频
        let mut reader = hound::WavReader::open(audio_path).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
            reader.samples::<f32>().map(|s| s.unwrap()).collect()
        } else {
            reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
        };

        // 创建流并处理
        let stream = recognizer.create_stream();
        stream.accept_waveform(spec.sample_rate as i32, &samples);
        
        recognizer.decode(&stream);
        let result = stream.get_result();
        
        Ok(result.map(|r| r.text).unwrap_or_default())
    }
}

/// ASR 客户端，封装 HTTP 和本地两种 Provider
pub struct AsrClient {
    pub http_provider: HttpAsrProvider,
    pub local_provider: LocalSherpaAsrProvider,
}

impl AsrClient {
    pub fn new(url: &str) -> Self {
        Self {
            http_provider: HttpAsrProvider::new(url),
            local_provider: LocalSherpaAsrProvider::new(),
        }
    }

    /// 切换本地模型
    pub fn set_local_model(&self, name: &str) {
        self.local_provider.set_model(name);
    }

    /// 根据 use_local 选择本地或远程识别
    pub async fn transcribe(&self, audio_path: &Path, language: &str, use_local: bool) -> Result<String, String> {
        if use_local {
            self.local_provider.transcribe(audio_path, language).await
        } else {
            self.http_provider.transcribe(audio_path, language).await
        }
    }
}
