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
    let fixed_len = "<|im_start|>system\n<|im_end|>\n<|im_start|>user\n<|audio_start|><|audio_end|><|im_end|>\n<|im_start|>assistant\n".len();
    fixed_len
        .checked_add(system_prompt.len())
        .and_then(|len| len.checked_add(audio_pads.len()))
        .and_then(|len| len.checked_add(assistant_prefill.len()))
        .ok_or_else(|| unprocessable("ASR prompt length overflow"))?;
    let prompt_text = format!(
        "<|im_start|>system\n{}<|im_end|>\n\
         <|im_start|>user\n<|audio_start|>{}<|audio_end|><|im_end|>\n\
         <|im_start|>assistant\n{}",
        system_prompt, audio_pads, assistant_prefill,
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
    let token_ids = tokenizer.encode(
        &prompt_text,
        EncodeOptions {
            add_special: false,
            parse_special: true,
        },
    );
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
    positions.extend((0..token_ids.len()).map(|index| [index; 4]));
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
    use crate::model::{MetaValue, MetaValueType};
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
        let mut token_types = vec![MetaValue::Uint32(1); 256];
        token_types.extend((0..SPECIAL_LITERALS.len()).map(|_| MetaValue::Uint32(3)));
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
                MetaValue::Array(MetaValueType::String, Vec::new()),
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
    fn prompt_retains_empty_system_framing_and_places_optional_prompt_inside_it() {
        let tokenizer = tokenizer();
        let (_, empty) = decoded_prompt(&tokenizer, 1, None, None);
        assert_eq!(
            empty,
            "<|im_start|>system\n<|im_end|>\n<|im_start|>user\n<|audio_start|><|audio_pad|><|audio_end|><|im_end|>\n<|im_start|>assistant\n"
        );

        let (_, populated) = decoded_prompt(&tokenizer, 1, Some("Use names exactly."), None);
        assert_eq!(
            populated,
            "<|im_start|>system\nUse names exactly.<|im_end|>\n<|im_start|>user\n<|audio_start|><|audio_pad|><|audio_end|><|im_end|>\n<|im_start|>assistant\n"
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
    fn prompt_has_exact_audio_pad_count_prefill_and_four_axis_positions() {
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
        assert_eq!(
            prompt.positions,
            (0..prompt.token_ids.len())
                .map(|index| [index; 4])
                .collect::<Vec<_>>()
        );
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
}
