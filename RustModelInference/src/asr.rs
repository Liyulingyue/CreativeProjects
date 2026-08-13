use crate::ggufrs::{ComponentRole, GgufrsFile};
use crate::model::{MetaValue, TensorSource};
use crate::qwen3::{Qwen3GenerateOptions, Qwen3Input, Qwen3Model};
use crate::qwen3a::{
    decode_pcm16_wav, log_mel_windows, validate_qwen3a_source, AsrAudioError, AudioEmbeddings,
    Qwen3AudioModel,
};
use crate::tokenizer::{BPETokenizer, EncodeOptions};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

const LANGUAGES: &[(&str, &str)] = &[
    ("Chinese", "zh"),
    ("English", "en"),
    ("Cantonese", "yue"),
    ("Arabic", "ar"),
    ("German", "de"),
    ("French", "fr"),
    ("Spanish", "es"),
    ("Portuguese", "pt"),
    ("Indonesian", "id"),
    ("Italian", "it"),
    ("Korean", "ko"),
    ("Russian", "ru"),
    ("Thai", "th"),
    ("Vietnamese", "vi"),
    ("Japanese", "ja"),
    ("Turkish", "tr"),
    ("Hindi", "hi"),
    ("Malay", "ms"),
    ("Dutch", "nl"),
    ("Swedish", "sv"),
    ("Danish", "da"),
    ("Finnish", "fi"),
    ("Polish", "pl"),
    ("Czech", "cs"),
    ("Filipino", "fil"),
    ("Persian", "fa"),
    ("Greek", "el"),
    ("Romanian", "ro"),
    ("Hungarian", "hu"),
    ("Macedonian", "mk"),
];

const DETECTED_ONLY_LANGUAGES: &[&str] = &[
    "Anhui",
    "Dongbei",
    "Fujian",
    "Gansu",
    "Guizhou",
    "Hebei",
    "Henan",
    "Hubei",
    "Hunan",
    "Jiangxi",
    "Ningxia",
    "Shandong",
    "Shaanxi",
    "Shanxi",
    "Sichuan",
    "Tianjin",
    "Yunnan",
    "Zhejiang",
    "Cantonese (Hong Kong accent)",
    "Cantonese (Guangdong accent)",
    "Wu language",
    "Minnan language",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrErrorKind {
    UnsupportedAudio,
    Unprocessable,
    Internal,
}

#[derive(Debug)]
pub struct AsrError {
    pub kind: AsrErrorKind,
    pub message: String,
}

impl std::fmt::Display for AsrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AsrError {}

#[derive(Debug, Clone)]
pub struct TranscriptionOptions {
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub max_new_tokens: usize,
}

impl Default for TranscriptionOptions {
    fn default() -> Self {
        Self {
            language: None,
            prompt: None,
            max_new_tokens: 256,
        }
    }
}

pub struct Transcription {
    pub text: String,
    pub language: Option<String>,
    pub token_ids: Vec<u32>,
    pub prompt_tokens: usize,
    pub audio_tokens: usize,
}

pub struct AsrRuntime {
    decoder: Arc<Qwen3Model>,
    audio: Qwen3AudioModel,
}

pub fn open_bundled_audio_source(
    model_path: &Path,
) -> Result<Option<Arc<dyn TensorSource>>, String> {
    let mut file = File::open(model_path)
        .map_err(|error| format!("Failed to open {}: {error}", model_path.display()))?;
    let mut magic = [0; 8];
    file.read_exact(&mut magic)
        .map_err(|error| format!("Failed to read {} magic: {error}", model_path.display()))?;
    if &magic != b"GGUFRS\0\0" {
        return Ok(None);
    }

    let package = GgufrsFile::open(model_path).map_err(|error| error.to_string())?;
    if package.component_id(ComponentRole::Mmproj).is_none() {
        return Ok(None);
    }
    let source = package
        .load_component(ComponentRole::Mmproj)
        .map_err(|error| error.to_string())?;
    match source.metadata("clip.has_audio_encoder") {
        None | Some(MetaValue::Bool(false)) => return Ok(None),
        Some(MetaValue::Bool(true)) => {}
        Some(_) => return Err("Invalid clip.has_audio_encoder: expected bool".into()),
    }
    match source.metadata("clip.audio.projector_type") {
        Some(MetaValue::String(value)) if value == "qwen3a" => {}
        _ => return Err("Invalid clip.audio.projector_type: expected qwen3a".into()),
    }
    validate_qwen3a_source(&source)?;
    Ok(Some(Arc::new(source)))
}

impl AsrRuntime {
    pub fn new(
        decoder: Arc<Qwen3Model>,
        audio_source: Arc<dyn TensorSource>,
    ) -> Result<Self, AsrError> {
        let audio = Qwen3AudioModel::from_source(audio_source, decoder.pool()).map_err(internal)?;
        if audio.config().projection != decoder.config().n_embd {
            return Err(internal(format!(
                "Audio projection width {} does not match decoder embedding width {}",
                audio.config().projection,
                decoder.config().n_embd
            )));
        }
        Ok(Self { decoder, audio })
    }

    pub fn transcribe_wav(
        &self,
        wav: &[u8],
        options: &TranscriptionOptions,
    ) -> Result<Transcription, AsrError> {
        let forced_language = normalize_language(options.language.as_deref())?;
        if options.max_new_tokens == 0 {
            return Err(unprocessable("max_new_tokens must be greater than zero"));
        }
        if options
            .prompt
            .as_deref()
            .is_some_and(|prompt| self.decoder.tokenizer().contains_special_literal(prompt))
        {
            return Err(unprocessable(
                "System prompt contains a tokenizer control literal",
            ));
        }

        let samples = decode_pcm16_wav(wav).map_err(map_audio_error)?;
        let windows = log_mel_windows(&samples).map_err(map_audio_error)?;
        let audio = self.audio.encode(&windows).map_err(internal)?;
        let prompt = build_asr_prompt(
            self.decoder.tokenizer(),
            self.decoder.config().n_ctx,
            audio.tokens,
            options.prompt.as_deref(),
            forced_language,
        )?;
        validate_generation_context(
            prompt.token_ids.len(),
            options.max_new_tokens,
            self.decoder.config().n_ctx,
        )?;
        let embeddings = replace_audio_embeddings(&self.decoder, &prompt, &audio)?;
        let generation = self
            .decoder
            .generate_asr(
                Qwen3Input {
                    token_ids: &prompt.token_ids,
                    positions: &prompt.positions,
                    embeddings: Some(&embeddings),
                },
                Qwen3GenerateOptions {
                    max_new_tokens: options.max_new_tokens,
                    temperature: 0.0,
                },
            )
            .map_err(internal)?;
        let (text, language) = parse_model_output(&generation.text, forced_language)?;
        Ok(Transcription {
            text,
            language,
            token_ids: generation.token_ids,
            prompt_tokens: generation.prompt_tokens,
            audio_tokens: audio.tokens,
        })
    }
}

fn validate_generation_context(
    prompt_tokens: usize,
    max_new_tokens: usize,
    decoder_context: usize,
) -> Result<(), AsrError> {
    let required = prompt_tokens
        .checked_add(max_new_tokens)
        .ok_or_else(|| unprocessable("ASR context length overflow"))?;
    if required > decoder_context {
        return Err(unprocessable(format!(
            "ASR requires {required} tokens; decoder context is {decoder_context}"
        )));
    }
    Ok(())
}

pub fn normalize_language(value: Option<&str>) -> Result<Option<&'static str>, AsrError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    LANGUAGES
        .iter()
        .find(|(name, code)| value.eq_ignore_ascii_case(name) || value.eq_ignore_ascii_case(code))
        .map(|(name, _)| Some(*name))
        .ok_or_else(|| unprocessable(format!("Unsupported ASR language: {value}")))
}

struct AsrPrompt {
    token_ids: Vec<u32>,
    positions: Vec<[usize; 4]>,
}

fn build_asr_prompt(
    tokenizer: &BPETokenizer,
    decoder_context: usize,
    audio_tokens: usize,
    system_prompt: Option<&str>,
    forced_language: Option<&'static str>,
) -> Result<AsrPrompt, AsrError> {
    if audio_tokens >= decoder_context {
        return Err(unprocessable(format!(
            "Audio token count {audio_tokens} must be below decoder context {decoder_context}"
        )));
    }
    let system_prompt = system_prompt.unwrap_or_default();
    if tokenizer.contains_special_literal(system_prompt) {
        return Err(unprocessable(
            "System prompt contains a tokenizer control literal",
        ));
    }

    let pad_len = "<|audio_pad|>"
        .len()
        .checked_mul(audio_tokens)
        .ok_or_else(|| unprocessable("Audio placeholder length overflow"))?;
    let audio_pads = "<|audio_pad|>".repeat(audio_tokens);
    if audio_pads.len() != pad_len {
        return Err(unprocessable("Audio placeholder length mismatch"));
    }
    let assistant_prefill = forced_language
        .map(|name| format!("language {name}<asr_text>"))
        .unwrap_or_default();
    let fixed_len = "<|im_start|>system\n<|im_end|>\n\n<|im_start|>user\n<|audio_start|><|audio_end|><|im_end|>\n<|im_start|>assistant\n".len();
    fixed_len
        .checked_add(system_prompt.len())
        .and_then(|len| len.checked_add(audio_pads.len()))
        .and_then(|len| len.checked_add(assistant_prefill.len()))
        .ok_or_else(|| unprocessable("ASR prompt length overflow"))?;
    let system_text = format!("<|im_start|>system\n{system_prompt}<|im_end|>\n");
    let user_text = format!(
        "\n<|im_start|>user\n<|audio_start|>{}<|audio_end|><|im_end|>\n\
         <|im_start|>assistant\n{}",
        audio_pads, assistant_prefill,
    );

    let semantic = |name| {
        tokenizer
            .special_token_id(name)
            .ok_or_else(|| internal(format!("Tokenizer is missing required {name} token")))
    };
    let im_start = semantic("im_start")?;
    let im_end = semantic("im_end")?;
    let audio_start = semantic("audio_start")?;
    let audio_pad = semantic("audio_pad")?;
    let audio_end = semantic("audio_end")?;
    let asr_text = semantic("asr_text")?;
    let encode_options = EncodeOptions {
        add_special: false,
        parse_special: true,
    };
    let mut token_ids = tokenizer.encode(&system_text, encode_options);
    token_ids.extend(tokenizer.encode(&user_text, encode_options));
    let count = |token| {
        token_ids
            .iter()
            .filter(|&&candidate| candidate == token)
            .count()
    };
    let start = token_ids.iter().position(|&token| token == audio_start);
    let end = token_ids.iter().position(|&token| token == audio_end);
    if count(im_start) != 3
        || count(im_end) != 2
        || count(audio_start) != 1
        || count(audio_end) != 1
        || count(audio_pad) != audio_tokens
        || count(asr_text) != usize::from(forced_language.is_some())
        || !matches!((start, end), (Some(start), Some(end)) if start < end
            && token_ids[start + 1..end].iter().all(|&token| token == audio_pad)
            && end - start - 1 == audio_tokens)
    {
        return Err(internal("Tokenizer violated the ASR prompt protocol"));
    }
    if token_ids.is_empty() || token_ids.len() > decoder_context {
        return Err(unprocessable("ASR prompt exceeds decoder context"));
    }
    let mut positions = Vec::new();
    positions
        .try_reserve_exact(token_ids.len())
        .map_err(|_| unprocessable("ASR position allocation failed"))?;
    positions.extend((0..token_ids.len()).map(|index| {
        if matches!((start, end), (Some(start), Some(end)) if start < index && index < end) {
            [index; 4]
        } else {
            [index, index, index, 0]
        }
    }));
    Ok(AsrPrompt {
        token_ids,
        positions,
    })
}

fn replace_audio_embeddings(
    decoder: &Qwen3Model,
    prompt: &AsrPrompt,
    audio: &AudioEmbeddings,
) -> Result<Vec<f32>, AsrError> {
    let dim = decoder.config().n_embd;
    let expected_audio_values = audio
        .tokens
        .checked_mul(audio.dim)
        .ok_or_else(|| internal("Audio embedding shape overflow"))?;
    if audio.dim != dim
        || audio.values.len() != expected_audio_values
        || audio.values.iter().any(|value| !value.is_finite())
        || prompt.positions.len() != prompt.token_ids.len()
    {
        return Err(internal("Invalid audio embedding protocol"));
    }
    let mut embeddings = decoder.embed_tokens(&prompt.token_ids).map_err(internal)?;
    let audio_pad = decoder
        .tokenizer()
        .special_token_id("audio_pad")
        .ok_or_else(|| internal("Tokenizer is missing required audio_pad token"))?;
    let pad_rows: Vec<usize> = prompt
        .token_ids
        .iter()
        .enumerate()
        .filter_map(|(index, &token)| (token == audio_pad).then_some(index))
        .collect();
    if pad_rows.len() != audio.tokens {
        return Err(internal(format!(
            "Audio pad count {} does not match audio token count {}",
            pad_rows.len(),
            audio.tokens
        )));
    }
    for (audio_row, prompt_row) in pad_rows.into_iter().enumerate() {
        let source_start = audio_row
            .checked_mul(dim)
            .ok_or_else(|| internal("Audio embedding offset overflow"))?;
        let source_end = source_start
            .checked_add(dim)
            .ok_or_else(|| internal("Audio embedding range overflow"))?;
        let destination_start = prompt_row
            .checked_mul(dim)
            .ok_or_else(|| internal("Decoder embedding offset overflow"))?;
        let destination_end = destination_start
            .checked_add(dim)
            .ok_or_else(|| internal("Decoder embedding range overflow"))?;
        let source = audio
            .values
            .get(source_start..source_end)
            .ok_or_else(|| internal("Invalid audio embedding range"))?;
        let destination = embeddings
            .get_mut(destination_start..destination_end)
            .ok_or_else(|| internal("Invalid decoder embedding range"))?;
        destination.copy_from_slice(source);
    }
    Ok(embeddings)
}

fn parse_model_output(
    output: &str,
    forced_language: Option<&'static str>,
) -> Result<(String, Option<String>), AsrError> {
    let output = trim_framing(output);
    if let Some(language) = forced_language {
        return Ok((output.to_string(), Some(language.to_string())));
    }
    let protocol = output
        .strip_prefix("language ")
        .ok_or_else(|| internal("ASR output is missing the language prefix"))?;
    let (language, transcript) = protocol
        .split_once("<asr_text>")
        .ok_or_else(|| internal("ASR output is missing the transcript marker"))?;
    let language = language.trim();
    let transcript = trim_framing(transcript).to_string();
    if language == "None" {
        if transcript.is_empty() {
            return Ok((transcript, None));
        }
        return Err(internal(
            "ASR returned language None with a nonempty transcript",
        ));
    }
    if !LANGUAGES.iter().any(|(name, _)| *name == language)
        && !DETECTED_ONLY_LANGUAGES.contains(&language)
    {
        return Err(internal(format!(
            "ASR returned unknown language: {language}"
        )));
    }
    Ok((transcript, Some(language.to_string())))
}

fn trim_framing(mut output: &str) -> &str {
    output = output.trim();
    loop {
        let trimmed = ["<|im_end|>", "<|endoftext|>"]
            .into_iter()
            .find_map(|marker| output.strip_suffix(marker));
        let Some(trimmed) = trimmed else {
            return output;
        };
        output = trimmed.trim_end();
    }
}

fn map_audio_error(error: AsrAudioError) -> AsrError {
    match error {
        AsrAudioError::Unsupported(message) => AsrError {
            kind: AsrErrorKind::UnsupportedAudio,
            message,
        },
        AsrAudioError::Invalid(message) => AsrError {
            kind: AsrErrorKind::Unprocessable,
            message,
        },
    }
}

fn unprocessable(message: impl Into<String>) -> AsrError {
    AsrError {
        kind: AsrErrorKind::Unprocessable,
        message: message.into(),
    }
}

fn internal(message: impl Into<String>) -> AsrError {
    AsrError {
        kind: AsrErrorKind::Internal,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ggufrs::{export_ggufrs, test_support, ExportOptions};
    #[cfg(feature = "parity-trace")]
    use crate::model::GGUFLoader;
    use crate::model::{MetaValue, MetaValueType};
    #[cfg(feature = "parity-trace")]
    use crate::thread_pool::ComputePool;
    #[cfg(feature = "parity-trace")]
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::sync::Arc;

    const SPECIAL_LITERALS: &[&str] = &[
        "<|im_start|>",
        "<|im_end|>",
        "<|audio_start|>",
        "<|audio_pad|>",
        "<|audio_end|>",
        "<asr_text>",
        "<|endoftext|>",
        "<tool_call>",
    ];

    #[cfg(feature = "parity-trace")]
    fn validate_metric_pair(got: &[f32], reference: &[f32]) -> Result<(), String> {
        if got.is_empty() || got.len() != reference.len() {
            return Err("metric inputs must be non-empty and equal length".into());
        }
        if got.iter().chain(reference).any(|value| !value.is_finite()) {
            return Err("metric inputs must be finite".into());
        }
        Ok(())
    }

    #[cfg(feature = "parity-trace")]
    fn nrmse(got: &[f32], reference: &[f32]) -> Result<f64, String> {
        validate_metric_pair(got, reference)?;
        let squared_error = got
            .iter()
            .zip(reference)
            .map(|(&got, &reference)| (f64::from(got) - f64::from(reference)).powi(2))
            .sum::<f64>();
        let squared_reference = reference
            .iter()
            .map(|&value| f64::from(value).powi(2))
            .sum::<f64>();
        Ok(squared_error.sqrt() / squared_reference.sqrt().max(1e-12))
    }

    #[cfg(feature = "parity-trace")]
    fn cosine(got: &[f32], reference: &[f32]) -> Result<f64, String> {
        validate_metric_pair(got, reference)?;
        let dot = got
            .iter()
            .zip(reference)
            .map(|(&got, &reference)| f64::from(got) * f64::from(reference))
            .sum::<f64>();
        let got_norm = got
            .iter()
            .map(|&value| f64::from(value).powi(2))
            .sum::<f64>();
        let reference_norm = reference
            .iter()
            .map(|&value| f64::from(value).powi(2))
            .sum::<f64>();
        Ok(dot / (got_norm * reference_norm).sqrt().max(1e-12))
    }

    #[cfg(feature = "parity-trace")]
    fn p99_abs(got: &[f32], reference: &[f32]) -> Result<f32, String> {
        validate_metric_pair(got, reference)?;
        let mut errors = got
            .iter()
            .zip(reference)
            .map(|(&got, &reference)| (got - reference).abs())
            .collect::<Vec<_>>();
        errors.sort_by(f32::total_cmp);
        Ok(errors[(99 * errors.len()).div_ceil(100) - 1])
    }

    #[cfg(feature = "parity-trace")]
    fn p99_scaled_abs(got: &[f32], reference: &[f32]) -> Result<f64, String> {
        validate_metric_pair(got, reference)?;
        let rms = (reference
            .iter()
            .map(|&value| f64::from(value).powi(2))
            .sum::<f64>()
            / reference.len() as f64)
            .sqrt();
        Ok(f64::from(p99_abs(got, reference)?) / rms.max(1e-6))
    }

    #[cfg(feature = "parity-trace")]
    fn row_cosines(got: &[f32], reference: &[f32], columns: usize) -> Result<Vec<f64>, String> {
        validate_metric_pair(got, reference)?;
        if columns == 0 || got.len() % columns != 0 {
            return Err("row cosine columns must divide the input length".into());
        }
        got.chunks_exact(columns)
            .zip(reference.chunks_exact(columns))
            .map(|(got, reference)| cosine(got, reference))
            .collect()
    }

    #[cfg(feature = "parity-trace")]
    fn top_k(values: &[f32], k: usize) -> Result<Vec<usize>, String> {
        if values.is_empty()
            || k == 0
            || k > values.len()
            || values.iter().any(|value| !value.is_finite())
        {
            return Err("top-k requires finite values and a valid k".into());
        }
        let mut ids = (0..values.len()).collect::<Vec<_>>();
        ids.sort_by(|&left, &right| {
            values[right]
                .total_cmp(&values[left])
                .then(left.cmp(&right))
        });
        ids.truncate(k);
        Ok(ids)
    }

    #[cfg(feature = "parity-trace")]
    #[derive(Debug)]
    struct TraceRecord {
        value: serde_json::Value,
        values: Option<Vec<f32>>,
    }

    #[cfg(feature = "parity-trace")]
    fn required_path(name: &str) -> Result<std::path::PathBuf, String> {
        std::env::var_os(name)
            .map(std::path::PathBuf::from)
            .ok_or_else(|| format!("{name} is required"))
    }

    #[cfg(feature = "parity-trace")]
    fn file_sha256(path: &Path) -> Result<String, String> {
        let mut file = File::open(path)
            .map_err(|error| format!("Failed to open {}: {error}", path.display()))?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)
            .map_err(|error| format!("Failed to hash {}: {error}", path.display()))?;
        Ok(format!("{:x}", hasher.finalize()))
    }

    #[cfg(feature = "parity-trace")]
    fn trace_records(path: &Path) -> Result<Vec<TraceRecord>, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        if contents.is_empty() {
            return Err(format!("{} is empty", path.display()));
        }
        contents
            .lines()
            .enumerate()
            .map(|(index, line)| {
                let value: serde_json::Value = serde_json::from_str(line).map_err(|error| {
                    format!(
                        "{} line {} is invalid JSON: {error}",
                        path.display(),
                        index + 1
                    )
                })?;
                let name = value["name"]
                    .as_str()
                    .ok_or_else(|| format!("{} line {} has no name", path.display(), index + 1))?;
                let values = value
                    .get("binary_path")
                    .map(|binary_path| {
                        let binary_path = binary_path.as_str().ok_or_else(|| {
                            format!("{name} binary_path in {} is not a string", path.display())
                        })?;
                        let bytes = std::fs::read(binary_path)
                            .map_err(|error| format!("Failed to read {binary_path}: {error}"))?;
                        if bytes.is_empty() || bytes.len() % 4 != 0 {
                            return Err(format!(
                                "{name} sidecar length {} is invalid",
                                bytes.len()
                            ));
                        }
                        let values = bytes
                            .chunks_exact(4)
                            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                            .collect::<Vec<_>>();
                        let shape = json_usizes(&value["shape"], &format!("{name} shape"))?;
                        let expected = shape.iter().try_fold(1usize, |length, &dimension| {
                            length
                                .checked_mul(dimension)
                                .ok_or_else(|| format!("{name} shape overflow"))
                        })?;
                        if value["len"].as_u64() != Some(values.len() as u64)
                            || expected != values.len()
                            || value["finite"] != true
                            || values.iter().any(|value| !value.is_finite())
                        {
                            return Err(format!("{name} sidecar does not match its JSON record"));
                        }
                        Ok(values)
                    })
                    .transpose()?;
                Ok(TraceRecord { value, values })
            })
            .collect()
    }

    #[cfg(feature = "parity-trace")]
    fn json_usizes(value: &serde_json::Value, context: &str) -> Result<Vec<usize>, String> {
        value
            .as_array()
            .ok_or_else(|| format!("{context} is not an array"))?
            .iter()
            .map(|value| {
                let value = value
                    .as_u64()
                    .ok_or_else(|| format!("{context} contains a non-integer"))?;
                usize::try_from(value).map_err(|_| format!("{context} exceeds usize"))
            })
            .collect()
    }

    #[cfg(feature = "parity-trace")]
    fn json_u32s(value: &serde_json::Value, context: &str) -> Result<Vec<u32>, String> {
        json_usizes(value, context)?
            .into_iter()
            .map(|value| u32::try_from(value).map_err(|_| format!("{context} exceeds u32")))
            .collect()
    }

    #[cfg(feature = "parity-trace")]
    fn named_records<'a>(records: &'a [TraceRecord], name: &str) -> Vec<&'a TraceRecord> {
        records
            .iter()
            .filter(|record| record.value["name"] == name)
            .collect()
    }

    #[cfg(feature = "parity-trace")]
    fn one_record<'a>(records: &'a [TraceRecord], name: &str) -> Result<&'a TraceRecord, String> {
        let records = named_records(records, name);
        if records.len() != 1 {
            return Err(format!("expected one {name} record, got {}", records.len()));
        }
        Ok(records[0])
    }

    #[cfg(feature = "parity-trace")]
    fn float_values<'a>(record: &'a TraceRecord, name: &str) -> Result<&'a [f32], String> {
        record
            .values
            .as_deref()
            .ok_or_else(|| format!("{name} has no F32 sidecar"))
    }

    #[cfg(feature = "parity-trace")]
    fn max_abs(got: &[f32], reference: &[f32]) -> Result<f32, String> {
        validate_metric_pair(got, reference)?;
        Ok(got
            .iter()
            .zip(reference)
            .map(|(&got, &reference)| (got - reference).abs())
            .fold(0.0, f32::max))
    }

    #[cfg(feature = "parity-trace")]
    fn compare_metric(
        name: &str,
        got: &[f32],
        reference: &[f32],
        minimum_cosine: Option<f64>,
        maximum_nrmse: Option<f64>,
        maximum_p99_abs: Option<f32>,
        maximum_abs: Option<f32>,
        maximum_p99_scaled_abs: Option<f64>,
    ) -> Result<(), String> {
        let cosine = cosine(got, reference)?;
        let nrmse = nrmse(got, reference)?;
        let p99_abs = p99_abs(got, reference)?;
        let max_abs = max_abs(got, reference)?;
        let p99_scaled_abs = p99_scaled_abs(got, reference)?;
        println!(
            "{name}: cosine={cosine:.9} nrmse={nrmse:.9} p99_abs={p99_abs:.9} max_abs={max_abs:.9} p99_scaled_abs={p99_scaled_abs:.9}"
        );
        if minimum_cosine.is_some_and(|limit| cosine < limit)
            || maximum_nrmse.is_some_and(|limit| nrmse > limit)
            || maximum_p99_abs.is_some_and(|limit| p99_abs > limit)
            || maximum_abs.is_some_and(|limit| max_abs > limit)
            || maximum_p99_scaled_abs.is_some_and(|limit| p99_scaled_abs > limit)
        {
            return Err(format!("{name} exceeded its fixed parity threshold"));
        }
        Ok(())
    }

    #[cfg(feature = "parity-trace")]
    fn compare_named_metric(
        name: &str,
        got_records: &[TraceRecord],
        reference_records: &[TraceRecord],
        minimum_cosine: Option<f64>,
        maximum_nrmse: Option<f64>,
        maximum_p99_abs: Option<f32>,
        maximum_abs: Option<f32>,
        maximum_p99_scaled_abs: Option<f64>,
    ) -> Result<(), String> {
        let got = named_records(got_records, name);
        let reference = named_records(reference_records, name);
        if got.len() != reference.len() || got.is_empty() {
            return Err(format!(
                "{name} occurrence mismatch: Rust {}, llama.cpp {}",
                got.len(),
                reference.len()
            ));
        }
        for (occurrence, (got, reference)) in got.into_iter().zip(reference).enumerate() {
            if got.value["shape"] != reference.value["shape"]
                || got.value["occurrence"] != reference.value["occurrence"]
            {
                return Err(format!("{name}[{occurrence}] shape/occurrence mismatch"));
            }
            compare_metric(
                &format!("{name}[{occurrence}]"),
                float_values(got, name)?,
                float_values(reference, name)?,
                minimum_cosine,
                maximum_nrmse,
                maximum_p99_abs,
                maximum_abs,
                maximum_p99_scaled_abs,
            )?;
        }
        Ok(())
    }

    #[cfg(feature = "parity-trace")]
    fn concatenate_named(records: &[TraceRecord], name: &str) -> Result<(Vec<f32>, usize), String> {
        let records = named_records(records, name);
        if records.is_empty() {
            return Err(format!("trace is missing {name}"));
        }
        let mut values = Vec::new();
        let mut columns = None;
        for record in records {
            let shape = json_usizes(&record.value["shape"], &format!("{name} shape"))?;
            if shape.len() != 2 {
                return Err(format!("{name} is not a matrix"));
            }
            if columns
                .replace(shape[1])
                .is_some_and(|previous| previous != shape[1])
            {
                return Err(format!("{name} column count changed between occurrences"));
            }
            values.extend_from_slice(float_values(record, name)?);
        }
        Ok((values, columns.unwrap()))
    }

    #[cfg(feature = "parity-trace")]
    fn p01_nonzero_row_cosine(
        got: &[f32],
        reference: &[f32],
        columns: usize,
    ) -> Result<f64, String> {
        validate_metric_pair(got, reference)?;
        if columns == 0 || got.len() % columns != 0 {
            return Err("row cosine columns must divide the input length".into());
        }
        let mut cosines = got
            .chunks_exact(columns)
            .zip(reference.chunks_exact(columns))
            .filter(|(_, reference)| reference.iter().any(|value| *value != 0.0))
            .map(|(got, reference)| cosine(got, reference))
            .collect::<Result<Vec<_>, _>>()?;
        if cosines.is_empty() {
            return Err("no nonzero reference rows".into());
        }
        cosines.sort_by(f64::total_cmp);
        Ok(cosines[cosines.len().div_ceil(100) - 1])
    }

    #[cfg(feature = "parity-trace")]
    fn run_qwen3_asr_trace_parity() -> Result<(), String> {
        let model_path = required_path("QWEN3_ASR_MODEL")?;
        let mmproj_path = required_path("QWEN3_ASR_MMPROJ")?;
        let wav_path = required_path("QWEN3_ASR_WAV")?;
        let reference_path = required_path("QWEN3_ASR_LLAMA_TRACE")?;
        let rust_path = required_path("RMI_PARITY_TRACE")?;
        for (path, size, hash) in [
            (
                &model_path,
                804_749_248,
                "bca259818b50ca7c4c05e9bdb35a5dc04fa039653a6d6f3f0f331f96f6aa1971",
            ),
            (
                &mmproj_path,
                214_392_480,
                "41a342b5e4c514e968cb756de6cd1b7be39eff43c44c57a2ef5fc6522e36603d",
            ),
            (
                &wav_path,
                481_718,
                "23775909b26f2ebb1ccf0b877e7590b2cc31700a94bccf2d4111b98e9595acd8",
            ),
        ] {
            let actual_size = std::fs::metadata(path)
                .map_err(|error| format!("Failed to stat {}: {error}", path.display()))?
                .len();
            if actual_size != size || file_sha256(path)? != hash {
                return Err(format!("fixed resource mismatch: {}", path.display()));
            }
        }
        if rust_path == reference_path {
            return Err("RMI_PARITY_TRACE must be fresh and differ from the reference path".into());
        }
        if rust_path.exists() {
            std::fs::remove_file(&rust_path)
                .map_err(|error| format!("Failed to clear {}: {error}", rust_path.display()))?;
        }

        let reference = trace_records(&reference_path)?;
        let expected_names = [
            "asr.pcm",
            "asr.raw_log_mel",
            "asr.normalized_mel",
            "asr.padded_mel",
            "asr.after_conv_blocks",
            "asr.after_conv_out",
            "asr.after_transformer",
            "asr.projected",
            "asr.prompt_ids",
            "asr.positions",
            "asr.decoder_first_logits",
            "asr.generated_ids",
        ];
        for name in expected_names {
            if named_records(&reference, name).is_empty() {
                return Err(format!("llama.cpp trace is missing {name}"));
            }
        }
        let reference_generated_ids = json_u32s(
            &one_record(&reference, "asr.generated_ids")?.value["token_ids"],
            "reference asr.generated_ids",
        )?;

        let reference_padded = named_records(&reference, "asr.padded_mel");
        let reference_chunks = reference_padded
            .iter()
            .map(|record| {
                let shape = json_usizes(&record.value["shape"], "asr.padded_mel shape")?;
                if shape.len() != 2 || shape[1] % 100 != 0 {
                    return Err(format!("invalid reference padded Mel shape {shape:?}"));
                }
                Ok(shape[1] / 100)
            })
            .sum::<Result<usize, String>>()?;
        for name in ["asr.after_conv_blocks", "asr.after_conv_out"] {
            if named_records(&reference, name).len() != reference_chunks {
                return Err(format!(
                    "llama.cpp {name} occurrence count does not match 100-frame chunks"
                ));
            }
        }
        for name in ["asr.after_transformer", "asr.projected"] {
            if named_records(&reference, name).len() != reference_padded.len() {
                return Err(format!(
                    "llama.cpp {name} occurrence count does not match Mel windows"
                ));
            }
        }

        let llm: Arc<dyn TensorSource> = Arc::new(
            GGUFLoader::from_file(&model_path)
                .map_err(|error| format!("Failed to load {}: {error}", model_path.display()))?,
        );
        let audio: Arc<dyn TensorSource> = Arc::new(
            GGUFLoader::from_file(&mmproj_path)
                .map_err(|error| format!("Failed to load {}: {error}", mmproj_path.display()))?,
        );
        let tokenizer = Arc::new(
            BPETokenizer::from_gguf_metadata(|key| llm.metadata(key).cloned())
                .map_err(|error| format!("Failed to load tokenizer: {error}"))?,
        );
        let decoder = Arc::new(Qwen3Model::from_source(
            llm,
            tokenizer,
            Arc::new(ComputePool::new(1)),
        )?);
        let runtime = AsrRuntime::new(decoder, audio).map_err(|error| error.to_string())?;
        let wav = std::fs::read(&wav_path)
            .map_err(|error| format!("Failed to read {}: {error}", wav_path.display()))?;
        let samples = decode_pcm16_wav(&wav)
            .map_err(map_audio_error)
            .map_err(|error| error.to_string())?;
        let windows = log_mel_windows(&samples)
            .map_err(map_audio_error)
            .map_err(|error| error.to_string())?;
        let audio = runtime.audio.encode(&windows)?;
        let prompt = build_asr_prompt(
            runtime.decoder.tokenizer(),
            runtime.decoder.config().n_ctx,
            audio.tokens,
            Some(""),
            None,
        )
        .map_err(|error| error.to_string())?;
        validate_generation_context(
            prompt.token_ids.len(),
            reference_generated_ids.len(),
            runtime.decoder.config().n_ctx,
        )
        .map_err(|error| error.to_string())?;
        let embeddings = replace_audio_embeddings(&runtime.decoder, &prompt, &audio)
            .map_err(|error| error.to_string())?;
        let generation = runtime.decoder.generate_asr(
            Qwen3Input {
                token_ids: &prompt.token_ids,
                positions: &prompt.positions,
                embeddings: Some(&embeddings),
            },
            Qwen3GenerateOptions {
                max_new_tokens: reference_generated_ids.len(),
                temperature: 0.0,
            },
        )?;
        let got = trace_records(&rust_path)?;

        if named_records(&got, "asr.pcm").len() != 1
            || float_values(one_record(&got, "asr.pcm")?, "asr.pcm")?
                .iter()
                .zip(float_values(one_record(&reference, "asr.pcm")?, "asr.pcm")?)
                .any(|(got, reference)| got.to_bits() != reference.to_bits())
            || one_record(&got, "asr.pcm")?.value["shape"]
                != one_record(&reference, "asr.pcm")?.value["shape"]
        {
            return Err("asr.pcm F32 bytes or sample count differ".into());
        }
        compare_named_metric(
            "asr.raw_log_mel",
            &got,
            &reference,
            None,
            None,
            Some(3e-4),
            Some(1e-3),
            None,
        )?;
        compare_named_metric(
            "asr.normalized_mel",
            &got,
            &reference,
            None,
            None,
            Some(1e-4),
            Some(5e-4),
            None,
        )?;

        let raw_shape = json_usizes(
            &one_record(&got, "asr.raw_log_mel")?.value["shape"],
            "asr.raw_log_mel shape",
        )?;
        if raw_shape.len() != 2 || raw_shape[0] != 128 || raw_shape[1] == 0 {
            return Err(format!("invalid raw Mel shape {raw_shape:?}"));
        }
        let expected_windows = raw_shape[1].div_ceil(800);
        let padded = named_records(&got, "asr.padded_mel");
        if padded.len() != expected_windows
            || padded.len() != named_records(&reference, "asr.padded_mel").len()
        {
            return Err("Mel window count differs from 800-frame boundaries".into());
        }
        let mut consumed_frames = 0usize;
        let normalized = float_values(
            one_record(&got, "asr.normalized_mel")?,
            "asr.normalized_mel",
        )?;
        for (index, record) in padded.iter().enumerate() {
            let shape = json_usizes(&record.value["shape"], "asr.padded_mel shape")?;
            if shape.len() != 2
                || shape[0] != 128
                || shape[1] == 0
                || shape[1] > 800
                || shape[1] % 100 != 0
            {
                return Err(format!("invalid padded Mel window {index} shape {shape:?}"));
            }
            let valid_frames = (raw_shape[1] - consumed_frames).min(800);
            let values = float_values(record, "asr.padded_mel")?;
            for mel in 0..128 {
                let source = &normalized[mel * raw_shape[1] + consumed_frames
                    ..mel * raw_shape[1] + consumed_frames + valid_frames];
                let row = &values[mel * shape[1]..(mel + 1) * shape[1]];
                if &row[..valid_frames] != source
                    || row[valid_frames..].iter().any(|value| *value != 0.0)
                {
                    return Err(format!("padded Mel window {index} data/padding mismatch"));
                }
            }
            consumed_frames += valid_frames;
        }
        if consumed_frames != raw_shape[1] {
            return Err("Mel windows do not cover every raw frame".into());
        }

        for (index, record) in named_records(&got, "asr.after_conv_blocks")
            .into_iter()
            .enumerate()
        {
            let shape = json_usizes(&record.value["shape"], "asr.after_conv_blocks shape")?;
            if shape != [1, 480, 16, 13] {
                return Err(format!(
                    "invalid after_conv_blocks[{index}] shape {shape:?}"
                ));
            }
        }
        for (index, record) in named_records(&got, "asr.after_conv_out")
            .into_iter()
            .enumerate()
        {
            let shape = json_usizes(&record.value["shape"], "asr.after_conv_out shape")?;
            if shape != [13, 896] {
                return Err(format!("invalid after_conv_out[{index}] shape {shape:?}"));
            }
        }
        for name in ["asr.after_transformer"] {
            for (record, padded) in named_records(&got, name).into_iter().zip(&padded) {
                let shape = json_usizes(&record.value["shape"], &format!("{name} shape"))?;
                let padded_shape = json_usizes(&padded.value["shape"], "asr.padded_mel shape")?;
                if shape != [padded_shape[1] / 100 * 13, 896] {
                    return Err(format!("invalid {name} window shape {shape:?}"));
                }
            }
        }

        compare_named_metric(
            "asr.after_conv_blocks",
            &got,
            &reference,
            Some(0.9999),
            Some(1.5e-2),
            None,
            None,
            Some(2e-2),
        )?;
        compare_named_metric(
            "asr.after_conv_out",
            &got,
            &reference,
            Some(0.9999),
            Some(2e-2),
            None,
            None,
            Some(2e-2),
        )?;
        for name in ["asr.after_transformer", "asr.projected"] {
            let (got_values, columns) = concatenate_named(&got, name)?;
            let (reference_values, reference_columns) = concatenate_named(&reference, name)?;
            if columns != reference_columns {
                return Err(format!("{name} column count differs"));
            }
            compare_metric(
                name,
                &got_values,
                &reference_values,
                Some(0.999),
                Some(4e-2),
                None,
                None,
                None,
            )?;
            let p01 = p01_nonzero_row_cosine(&got_values, &reference_values, columns)?;
            println!("{name}: nonzero-row p01 cosine={p01:.9}");
            if p01 < 0.99 {
                return Err(format!("{name} row cosine below 0.99"));
            }
        }

        let projected_shapes = named_records(&got, "asr.projected")
            .into_iter()
            .map(|record| json_usizes(&record.value["shape"], "asr.projected shape"))
            .collect::<Result<Vec<_>, _>>()?;
        let expected_audio_tokens = padded
            .iter()
            .map(|record| {
                let shape = json_usizes(&record.value["shape"], "asr.padded_mel shape")?;
                Ok(shape[1] / 100 * 13)
            })
            .sum::<Result<usize, String>>()?;
        if projected_shapes
            .iter()
            .any(|shape| shape.len() != 2 || shape[1] != 1024)
            || projected_shapes.iter().map(|shape| shape[0]).sum::<usize>() != expected_audio_tokens
            || audio.tokens != expected_audio_tokens
        {
            return Err("projected audio token count/order differs from 100-frame chunks".into());
        }
        let prompt_ids = json_u32s(
            &one_record(&got, "asr.prompt_ids")?.value["token_ids"],
            "asr.prompt_ids",
        )?;
        let reference_prompt_ids = json_u32s(
            &one_record(&reference, "asr.prompt_ids")?.value["token_ids"],
            "reference asr.prompt_ids",
        )?;
        let positions = json_usizes(
            &one_record(&got, "asr.positions")?.value["usize_values"],
            "asr.positions",
        )?;
        let audio_pad = runtime
            .decoder
            .tokenizer()
            .special_token_id("audio_pad")
            .ok_or("tokenizer has no audio_pad token")?;
        if prompt_ids != reference_prompt_ids
            || positions.len() != prompt_ids.len() * 4
            || positions
                .chunks_exact(4)
                .enumerate()
                .any(|(index, position)| {
                    let expected = if prompt_ids[index] == audio_pad {
                        [index; 4]
                    } else {
                        [index, index, index, 0]
                    };
                    position != expected
                })
        {
            return Err("prompt token IDs/order or segmented positions differ".into());
        }
        let pad_rows = prompt_ids
            .iter()
            .enumerate()
            .filter_map(|(index, &id)| (id == audio_pad).then_some(index))
            .collect::<Vec<_>>();
        if pad_rows.len() != expected_audio_tokens
            || pad_rows.windows(2).any(|rows| rows[1] != rows[0] + 1)
        {
            return Err("embedding-slot indices do not exactly cover projected audio rows".into());
        }

        let got_logits = float_values(
            one_record(&got, "asr.decoder_first_logits")?,
            "asr.decoder_first_logits",
        )?;
        let reference_logits = float_values(
            one_record(&reference, "asr.decoder_first_logits")?,
            "asr.decoder_first_logits",
        )?;
        validate_metric_pair(got_logits, reference_logits)?;
        let got_mean = got_logits
            .iter()
            .map(|&value| f64::from(value))
            .sum::<f64>()
            / got_logits.len() as f64;
        let reference_mean = reference_logits
            .iter()
            .map(|&value| f64::from(value))
            .sum::<f64>()
            / reference_logits.len() as f64;
        let centered_got = got_logits
            .iter()
            .map(|&value| (f64::from(value) - got_mean) as f32)
            .collect::<Vec<_>>();
        let centered_reference = reference_logits
            .iter()
            .map(|&value| (f64::from(value) - reference_mean) as f32)
            .collect::<Vec<_>>();
        compare_metric(
            "asr.decoder_first_logits centered",
            &centered_got,
            &centered_reference,
            Some(0.9995),
            Some(3e-2),
            Some(0.10),
            Some(0.30),
            None,
        )?;
        let got_top10 = top_k(got_logits, 10)?;
        let reference_top10 = top_k(reference_logits, 10)?;
        let overlap = got_top10
            .iter()
            .filter(|id| reference_top10.contains(id))
            .count();
        if overlap < 9 {
            return Err(format!("first-token top-10 overlap is {overlap}/10"));
        }
        let reference_margin =
            reference_logits[reference_top10[0]] - reference_logits[reference_top10[1]];
        if (reference_margin >= 0.10 && got_top10[0] != reference_top10[0])
            || (reference_margin < 0.10 && !reference_top10[..3].contains(&got_top10[0]))
        {
            return Err(format!(
                "first-token argmax rejected: Rust {}, reference {}, margin {reference_margin}",
                got_top10[0], reference_top10[0]
            ));
        }

        let generated_ids = json_u32s(
            &one_record(&got, "asr.generated_ids")?.value["token_ids"],
            "asr.generated_ids",
        )?;
        if generated_ids != reference_generated_ids || generated_ids != generation.token_ids {
            return Err("generated token IDs differ exactly".into());
        }
        println!("exact generated IDs: {generated_ids:?}");
        Ok(())
    }

    fn is_direct_byte(byte: u8) -> bool {
        matches!(byte, b'!'..=b'~' | 0xa1..=0xac | 0xae..=0xff)
    }

    fn byte_token(byte: u8) -> String {
        let codepoint = if is_direct_byte(byte) {
            u32::from(byte)
        } else {
            256 + (0..byte).filter(|value| !is_direct_byte(*value)).count() as u32
        };
        char::from_u32(codepoint).unwrap().to_string()
    }

    fn tokenizer() -> BPETokenizer {
        let mut tokens: Vec<String> = (0..=u8::MAX).map(byte_token).collect();
        tokens.extend(
            SPECIAL_LITERALS
                .iter()
                .map(|literal| (*literal).to_string()),
        );
        let newline = byte_token(b'\n');
        tokens.push(format!("{newline}{newline}"));
        let mut token_types = vec![MetaValue::Uint32(1); 256];
        token_types.extend((0..SPECIAL_LITERALS.len()).map(|_| MetaValue::Uint32(3)));
        token_types.push(MetaValue::Uint32(1));
        let eos_id = u32::try_from(256 + 6).unwrap();
        let metadata: HashMap<String, MetaValue> = HashMap::from([
            (
                "tokenizer.ggml.model".into(),
                MetaValue::String("gpt2".into()),
            ),
            (
                "tokenizer.ggml.pre".into(),
                MetaValue::String("qwen2".into()),
            ),
            (
                "tokenizer.ggml.tokens".into(),
                MetaValue::Array(
                    MetaValueType::String,
                    tokens.into_iter().map(MetaValue::String).collect(),
                ),
            ),
            (
                "tokenizer.ggml.token_type".into(),
                MetaValue::Array(MetaValueType::Uint32, token_types),
            ),
            (
                "tokenizer.ggml.merges".into(),
                MetaValue::Array(
                    MetaValueType::String,
                    vec![MetaValue::String(format!("{newline} {newline}"))],
                ),
            ),
            (
                "tokenizer.ggml.eos_token_id".into(),
                MetaValue::Uint32(eos_id),
            ),
        ]);
        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned()).unwrap()
    }

    fn decoded_prompt(
        tokenizer: &BPETokenizer,
        audio_tokens: usize,
        system_prompt: Option<&str>,
        forced_language: Option<&'static str>,
    ) -> (AsrPrompt, String) {
        let prompt = build_asr_prompt(
            tokenizer,
            4096,
            audio_tokens,
            system_prompt,
            forced_language,
        )
        .unwrap();
        let decoded = tokenizer.decode(&prompt.token_ids, true);
        (prompt, decoded)
    }

    fn overwrite_package_text(path: &std::path::Path, before: &[u8], after: &[u8]) {
        assert_eq!(before.len(), after.len());
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let mut prefix =
            vec![0; usize::try_from(file.metadata().unwrap().len().min(1 << 20)).unwrap()];
        file.read_exact(&mut prefix).unwrap();
        let positions = prefix
            .windows(before.len())
            .enumerate()
            .filter_map(|(position, bytes)| (bytes == before).then_some(position))
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 1, "expected one package metadata match");
        file.seek(SeekFrom::Start(positions[0] as u64)).unwrap();
        file.write_all(after).unwrap();
    }

    #[test]
    fn bundled_audio_source_resolution_is_strict() {
        let inputs = test_support::test_gguf_pair_with_arch("qwen3vl");
        assert!(open_bundled_audio_source(&inputs.llm).unwrap().is_none());

        let llm_only = inputs.dir.join("llm-only.ggufrs");
        export_ggufrs(&llm_only, &inputs.llm, None, ExportOptions::default()).unwrap();
        assert!(open_bundled_audio_source(&llm_only).unwrap().is_none());

        let vision = inputs.dir.join("vision.ggufrs");
        export_ggufrs(
            &vision,
            &inputs.llm,
            Some(&inputs.mmproj),
            ExportOptions::default(),
        )
        .unwrap();
        assert!(open_bundled_audio_source(&vision).unwrap().is_none());

        let audio = inputs.dir.join("audio.gguf");
        test_support::write_qwen3a_mmproj(&audio, "qwen3a");
        let package = inputs.dir.join("audio.ggufrs");
        export_ggufrs(
            &package,
            &inputs.llm,
            Some(&audio),
            ExportOptions::default(),
        )
        .unwrap();
        let source = open_bundled_audio_source(&package).unwrap().unwrap();
        assert_eq!(
            source
                .metadata("clip.audio.projector_type")
                .and_then(MetaValue::to_string_val),
            Some("qwen3a")
        );
        drop(source);

        overwrite_package_text(&package, b"qwen3a", b"other!");
        assert!(open_bundled_audio_source(&package)
            .err()
            .unwrap()
            .contains("clip.audio.projector_type"));
        overwrite_package_text(&package, b"other!", b"qwen3a");
        overwrite_package_text(
            &package,
            b"clip.audio.projector_type",
            b"clip.audio.projector_typx",
        );
        assert!(open_bundled_audio_source(&package)
            .err()
            .unwrap()
            .contains("clip.audio.projector_type"));

        let malformed = inputs.dir.join("malformed.ggufrs");
        std::fs::write(&malformed, b"GGUFRS\0\0").unwrap();
        assert!(open_bundled_audio_source(&malformed).is_err());
    }

    #[test]
    fn normalizes_only_the_supported_language_names_and_codes() {
        for &(canonical, code) in LANGUAGES {
            let canonical_input = format!("  {}  ", canonical.to_ascii_uppercase());
            let code_input = format!("  {}  ", code.to_ascii_uppercase());
            assert_eq!(
                normalize_language(Some(&canonical_input)).unwrap(),
                Some(canonical)
            );
            assert_eq!(
                normalize_language(Some(&code_input)).unwrap(),
                Some(canonical)
            );
        }
        assert_eq!(normalize_language(None).unwrap(), None);
        assert_eq!(normalize_language(Some("")).unwrap(), None);
        assert_eq!(normalize_language(Some(" \n\t ")).unwrap(), None);

        for rejected in [
            "auto", "en-US", "zh-Hans", "cmn", "tl", "cn", "jp", "Hebrew",
        ]
        .into_iter()
        .chain(DETECTED_ONLY_LANGUAGES.iter().copied())
        {
            assert_eq!(
                normalize_language(Some(rejected)).unwrap_err().kind,
                AsrErrorKind::Unprocessable,
                "accepted {rejected:?}"
            );
        }
    }

    #[test]
    fn prompt_matches_the_two_call_oracle_separator() {
        let tokenizer = tokenizer();
        let prompt = build_asr_prompt(&tokenizer, 4096, 1, None, None).unwrap();
        let encode = |text| {
            tokenizer.encode(
                text,
                EncodeOptions {
                    add_special: false,
                    parse_special: true,
                },
            )
        };
        let mut expected = encode("<|im_start|>system\n<|im_end|>\n");
        expected.extend(encode(
            "\n<|im_start|>user\n<|audio_start|><|audio_pad|><|audio_end|><|im_end|>\n<|im_start|>assistant\n",
        ));

        assert_eq!(prompt.token_ids, expected);
    }

    #[test]
    fn prompt_retains_empty_system_framing_and_places_optional_prompt_inside_it() {
        let tokenizer = tokenizer();
        let (_, empty) = decoded_prompt(&tokenizer, 1, None, None);
        assert_eq!(
            empty,
            "<|im_start|>system\n<|im_end|>\n\n<|im_start|>user\n<|audio_start|><|audio_pad|><|audio_end|><|im_end|>\n<|im_start|>assistant\n"
        );

        let (_, populated) = decoded_prompt(&tokenizer, 1, Some("Use names exactly."), None);
        assert_eq!(
            populated,
            "<|im_start|>system\nUse names exactly.<|im_end|>\n\n<|im_start|>user\n<|audio_start|><|audio_pad|><|audio_end|><|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn prompt_rejects_every_tokenizer_special_literal() {
        let tokenizer = tokenizer();
        for literal in SPECIAL_LITERALS {
            let prompt = format!("before {literal} after");
            assert_eq!(
                build_asr_prompt(&tokenizer, 4096, 1, Some(&prompt), None)
                    .err()
                    .unwrap()
                    .kind,
                AsrErrorKind::Unprocessable,
                "accepted {literal}"
            );
        }
    }

    #[test]
    fn prompt_has_exact_audio_pad_count_prefill_and_segmented_positions() {
        let tokenizer = tokenizer();
        let (prompt, decoded) = decoded_prompt(&tokenizer, 3, None, Some("English"));
        let audio_start = tokenizer.special_token_id("audio_start").unwrap();
        let audio_pad = tokenizer.special_token_id("audio_pad").unwrap();
        let audio_end = tokenizer.special_token_id("audio_end").unwrap();
        let start = prompt
            .token_ids
            .iter()
            .position(|&token| token == audio_start)
            .unwrap();
        let end = prompt
            .token_ids
            .iter()
            .position(|&token| token == audio_end)
            .unwrap();
        assert_eq!(&prompt.token_ids[start + 1..end], &[audio_pad; 3]);
        assert_eq!(
            prompt
                .token_ids
                .iter()
                .filter(|&&token| token == audio_pad)
                .count(),
            3
        );
        assert!(decoded.ends_with("<|im_start|>assistant\nlanguage English<asr_text>"));
        assert_eq!(prompt.positions[start], [start, start, start, 0]);
        for index in start + 1..end {
            assert_eq!(prompt.positions[index], [index; 4]);
        }
        assert_eq!(prompt.positions[end], [end, end, end, 0]);
    }

    #[test]
    fn prompt_rejects_audio_count_at_or_above_decoder_context() {
        let tokenizer = tokenizer();
        for count in [8, usize::MAX] {
            assert_eq!(
                build_asr_prompt(&tokenizer, 8, count, None, None)
                    .err()
                    .unwrap()
                    .kind,
                AsrErrorKind::Unprocessable
            );
        }
    }

    #[test]
    fn prompt_and_generation_context_reject_overflow_and_excess() {
        let tokenizer = tokenizer();
        assert_eq!(
            build_asr_prompt(&tokenizer, 8, 1, None, None)
                .err()
                .unwrap()
                .kind,
            AsrErrorKind::Unprocessable
        );
        for (prompt, generation, context) in [(7, 2, 8), (usize::MAX, 1, usize::MAX)] {
            assert_eq!(
                validate_generation_context(prompt, generation, context)
                    .unwrap_err()
                    .kind,
                AsrErrorKind::Unprocessable
            );
        }
    }

    #[test]
    fn audio_replacement_changes_only_pad_rows() {
        let tokenizer = Arc::new(tokenizer());
        let decoder = crate::qwen3::test_model(Arc::clone(&tokenizer), 4096, 32);
        let prompt = build_asr_prompt(&tokenizer, 4096, 2, None, None).unwrap();
        let original = decoder.embed_tokens(&prompt.token_ids).unwrap();
        let audio = AudioEmbeddings {
            values: [vec![1.25; 32], vec![-2.5; 32]].concat(),
            tokens: 2,
            dim: 32,
        };
        let replaced = replace_audio_embeddings(&decoder, &prompt, &audio).unwrap();
        let audio_pad = tokenizer.special_token_id("audio_pad").unwrap();
        let mut audio_row = 0;
        for (index, &token_id) in prompt.token_ids.iter().enumerate() {
            let range = index * 32..(index + 1) * 32;
            if token_id == audio_pad {
                assert_eq!(
                    &replaced[range],
                    &audio.values[audio_row * 32..(audio_row + 1) * 32]
                );
                audio_row += 1;
            } else {
                assert_eq!(&replaced[range.clone()], &original[range]);
            }
        }
        assert_eq!(audio_row, 2);
    }

    #[test]
    fn audio_replacement_rejects_count_dimension_value_and_finite_mismatches() {
        let tokenizer = Arc::new(tokenizer());
        let decoder = crate::qwen3::test_model(Arc::clone(&tokenizer), 4096, 32);
        let prompt = build_asr_prompt(&tokenizer, 4096, 2, None, None).unwrap();
        for audio in [
            AudioEmbeddings {
                values: vec![0.0; 32],
                tokens: 1,
                dim: 32,
            },
            AudioEmbeddings {
                values: vec![0.0; 62],
                tokens: 2,
                dim: 31,
            },
            AudioEmbeddings {
                values: vec![0.0; 63],
                tokens: 2,
                dim: 32,
            },
            AudioEmbeddings {
                values: {
                    let mut values = vec![0.0; 64];
                    values[4] = f32::NAN;
                    values
                },
                tokens: 2,
                dim: 32,
            },
        ] {
            assert_eq!(
                replace_audio_embeddings(&decoder, &prompt, &audio)
                    .unwrap_err()
                    .kind,
                AsrErrorKind::Internal
            );
        }
    }

    #[test]
    fn parses_auto_language_protocol_and_detected_only_labels() {
        assert_eq!(
            parse_model_output("language English<asr_text>Hello", None).unwrap(),
            ("Hello".into(), Some("English".into()))
        );
        assert_eq!(
            parse_model_output("language English \n\t <asr_text> Hello \n", None).unwrap(),
            ("Hello".into(), Some("English".into()))
        );
        for label in DETECTED_ONLY_LANGUAGES {
            let output = format!("language {label}<asr_text>ok");
            assert_eq!(
                parse_model_output(&output, None).unwrap(),
                ("ok".into(), Some((*label).into()))
            );
        }
    }

    #[test]
    fn auto_language_none_requires_an_empty_transcript() {
        assert_eq!(
            parse_model_output("language None<asr_text>  \n", None).unwrap(),
            (String::new(), None)
        );
        assert_eq!(
            parse_model_output("language None<asr_text>words", None)
                .unwrap_err()
                .kind,
            AsrErrorKind::Internal
        );
    }

    #[test]
    fn auto_language_protocol_rejects_missing_or_unknown_fields() {
        for output in [
            "English<asr_text>Hello",
            "language English Hello",
            "language Klingon<asr_text>Hello",
        ] {
            assert_eq!(
                parse_model_output(output, None).unwrap_err().kind,
                AsrErrorKind::Internal,
                "accepted {output:?}"
            );
        }
    }

    #[test]
    fn forced_output_only_trims_framing_and_outer_whitespace() {
        assert_eq!(
            parse_model_output(" \n hello hello \n<|im_end|> \n", Some("English")).unwrap(),
            ("hello hello".into(), Some("English".into()))
        );
        assert_eq!(
            parse_model_output("word word word", Some("English"))
                .unwrap()
                .0,
            "word word word"
        );
    }

    #[test]
    fn transcription_options_default_to_greedy_256_token_generation() {
        let options = TranscriptionOptions::default();
        assert_eq!(options.language, None);
        assert_eq!(options.prompt, None);
        assert_eq!(options.max_new_tokens, 256);
    }

    #[cfg(feature = "parity-trace")]
    #[test]
    fn parity_metrics_match_hand_calculated_vectors() {
        let got = [1.0, 2.0, 3.0];
        let reference = [1.0, 2.0, 4.0];
        assert!((nrmse(&got, &reference).unwrap() - (1.0f64 / 21.0).sqrt()).abs() < 1e-12);
        assert!((cosine(&got, &reference).unwrap() - 17.0 / (14.0f64 * 21.0).sqrt()).abs() < 1e-12);
        assert_eq!(p99_abs(&got, &reference).unwrap(), 1.0);
        assert!((p99_scaled_abs(&got, &reference).unwrap() - 1.0 / 7.0f64.sqrt()).abs() < 1e-12);
        assert_eq!(
            row_cosines(&got, &reference, 3).unwrap(),
            vec![cosine(&got, &reference).unwrap()]
        );
    }

    #[cfg(feature = "parity-trace")]
    #[test]
    fn parity_metrics_reject_invalid_inputs_and_rank_ties_by_token_id() {
        let invalid_pairs: &[(&[f32], &[f32])] = &[
            (&[], &[]),
            (&[1.0], &[]),
            (&[f32::NAN], &[1.0]),
            (&[1.0], &[f32::INFINITY]),
        ];
        for (got, reference) in invalid_pairs {
            assert!(nrmse(got, reference).is_err());
            assert!(cosine(got, reference).is_err());
            assert!(p99_abs(got, reference).is_err());
            assert!(p99_scaled_abs(got, reference).is_err());
        }
        assert!(row_cosines(&[1.0], &[1.0], 0).is_err());
        assert!(row_cosines(&[1.0, 2.0], &[1.0, 2.0], 3).is_err());
        assert!(row_cosines(&[1.0, f32::NAN], &[1.0, 2.0], 1).is_err());
        assert!(top_k(&[], 1).is_err());
        assert!(top_k(&[1.0], 0).is_err());
        assert!(top_k(&[1.0], 2).is_err());
        assert!(top_k(&[f32::NAN], 1).is_err());
        assert_eq!(top_k(&[1.0, 2.0, 2.0, 0.0], 3).unwrap(), vec![1, 2, 0]);
    }

    #[cfg(feature = "parity-trace")]
    #[test]
    #[ignore = "requires the fixed Qwen3-ASR GGUFs, WAV, and llama.cpp trace"]
    fn qwen3_asr_matches_pinned_llama_cpp_trace() {
        run_qwen3_asr_trace_parity().unwrap();
    }
}
