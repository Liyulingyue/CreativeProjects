use std::collections::HashMap;
use std::ops::Range;

use unicode_categories::UnicodeCategories;

use crate::model::{MetaValue, MetaValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    pub add_special: bool,
    pub parse_special: bool,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            add_special: false,
            parse_special: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreTokenizer {
    Qwen2,
    Qwen35,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenType {
    Normal,
    Unknown,
    Control,
    UserDefined,
    Unused,
    Byte,
}

#[derive(Debug, Clone)]
struct SpecialToken {
    text: String,
    id: u32,
    kind: TokenType,
}

#[derive(Debug)]
pub struct BPETokenizer {
    tokens: Vec<String>,
    token_types: Vec<TokenType>,
    token_to_id: HashMap<String, u32>,
    merge_ranks: HashMap<(String, String), u32>,
    byte_encoder: Vec<String>,
    byte_decoder: HashMap<char, u8>,
    pre: PreTokenizer,
    special_tokens: Vec<SpecialToken>,
    semantic_tokens: HashMap<String, u32>,
    bos_id: Option<u32>,
    eos_id: Option<u32>,
    add_bos: bool,
    add_eos: bool,
    byte_fallback: bool,
}

const QWEN_SEMANTIC_TOKENS: &[(&str, &str)] = &[
    ("<|im_start|>", "im_start"),
    ("<|im_end|>", "im_end"),
    ("<|image_pad|>", "image_pad"),
    ("<|vision_pad|>", "vision_pad"),
    ("<|vision_start|>", "vision_start"),
    ("<|vision_end|>", "vision_end"),
    ("<|audio_start|>", "audio_start"),
    ("<|audio_pad|>", "audio_pad"),
    ("<|audio_end|>", "audio_end"),
    ("<asr_text>", "asr_text"),
    ("<|endoftext|>", "endoftext"),
];

fn string_array(value: Option<MetaValue>, key: &str) -> Result<Vec<String>, String> {
    let Some(MetaValue::Array(MetaValueType::String, values)) = value else {
        return Err(format!("Missing or invalid {key}: expected string array"));
    };
    values
        .into_iter()
        .map(|value| match value {
            MetaValue::String(value) => Ok(value),
            _ => Err(format!("Invalid {key}: expected string array entries")),
        })
        .collect()
}

fn integer_array(value: Option<MetaValue>, key: &str) -> Result<Vec<u64>, String> {
    let Some(MetaValue::Array(kind, values)) = value else {
        return Err(format!("Missing or invalid {key}: expected integer array"));
    };
    if !matches!(
        kind,
        MetaValueType::Uint8
            | MetaValueType::Int8
            | MetaValueType::Uint16
            | MetaValueType::Int16
            | MetaValueType::Uint32
            | MetaValueType::Int32
            | MetaValueType::Uint64
            | MetaValueType::Int64
    ) {
        return Err(format!("Invalid {key}: expected integer array"));
    }
    values
        .into_iter()
        .map(|value| {
            value
                .to_u64()
                .ok_or_else(|| format!("Invalid {key}: expected integer array entries"))
        })
        .collect()
}

fn token_type(value: u64) -> Result<TokenType, String> {
    match value {
        1 => Ok(TokenType::Normal),
        2 => Ok(TokenType::Unknown),
        3 => Ok(TokenType::Control),
        4 => Ok(TokenType::UserDefined),
        5 => Ok(TokenType::Unused),
        6 => Ok(TokenType::Byte),
        _ => Err(format!("Unsupported tokenizer token type: {value}")),
    }
}

fn optional_token_id(value: Option<MetaValue>, key: &str) -> Result<Option<u32>, String> {
    match value {
        None => Ok(None),
        Some(value) => value
            .to_u64()
            .and_then(|id| u32::try_from(id).ok())
            .map(Some)
            .ok_or_else(|| format!("Invalid {key}")),
    }
}

fn validate_token_id(id: Option<u32>, vocab_size: usize, key: &str) -> Result<(), String> {
    if id.is_some_and(|id| id as usize >= vocab_size) {
        return Err(format!("Invalid {key}: token ID is outside the vocabulary"));
    }
    Ok(())
}

fn bool_meta(value: Option<MetaValue>, key: &str) -> Result<bool, String> {
    match value {
        None => Ok(false),
        Some(MetaValue::Bool(value)) => Ok(value),
        Some(_) => Err(format!("Invalid {key}: expected bool")),
    }
}

impl BPETokenizer {
    pub fn from_gguf_metadata(
        get_meta: impl Fn(&str) -> Option<MetaValue>,
    ) -> Result<Self, String> {
        match get_meta("tokenizer.ggml.model") {
            Some(MetaValue::String(value)) if value == "gpt2" => {}
            Some(MetaValue::String(value)) => {
                return Err(format!(
                    "Unsupported tokenizer.ggml.model {value:?}; expected gpt2"
                ));
            }
            _ => return Err("Missing or invalid tokenizer.ggml.model".into()),
        }

        let pre = match get_meta("tokenizer.ggml.pre") {
            Some(MetaValue::String(value)) if value == "qwen2" => PreTokenizer::Qwen2,
            Some(MetaValue::String(value)) if value == "qwen35" => PreTokenizer::Qwen35,
            Some(MetaValue::String(value)) => {
                return Err(format!(
                    "Unsupported tokenizer.ggml.pre {value:?}; expected qwen2 or qwen35"
                ));
            }
            _ => return Err("Missing or invalid tokenizer.ggml.pre".into()),
        };

        let tokens = string_array(get_meta("tokenizer.ggml.tokens"), "tokenizer.ggml.tokens")?;

        let token_type_values = integer_array(
            get_meta("tokenizer.ggml.token_type"),
            "tokenizer.ggml.token_type",
        )?;
        if token_type_values.len() != tokens.len() {
            return Err(format!(
                "tokenizer.ggml.token_type length {} does not match token length {}",
                token_type_values.len(),
                tokens.len()
            ));
        }
        let token_types = token_type_values
            .into_iter()
            .map(token_type)
            .collect::<Result<Vec<_>, _>>()?;

        let merges = string_array(get_meta("tokenizer.ggml.merges"), "tokenizer.ggml.merges")?;

        let mut token_to_id = HashMap::new();
        for (i, t) in tokens.iter().enumerate() {
            token_to_id.insert(t.clone(), i as u32);
        }

        let mut merge_ranks = HashMap::new();
        for (rank, merge) in merges.into_iter().enumerate() {
            let Some((left, right)) = merge.split_once(' ') else {
                return Err(format!("Invalid tokenizer.ggml.merges entry: {merge:?}"));
            };
            merge_ranks.insert((left.to_string(), right.to_string()), rank as u32);
        }

        let byte_encoder = build_byte_encoder();
        let mut byte_decoder = HashMap::new();
        for (b, s) in byte_encoder.iter().enumerate() {
            if !s.is_empty() {
                if let Some(ch) = s.chars().next() {
                    byte_decoder.insert(ch, b as u8);
                }
            }
        }

        let bos_id = optional_token_id(
            get_meta("tokenizer.ggml.bos_token_id"),
            "tokenizer.ggml.bos_token_id",
        )?;
        let eos_id = optional_token_id(
            get_meta("tokenizer.ggml.eos_token_id"),
            "tokenizer.ggml.eos_token_id",
        )?;
        validate_token_id(bos_id, tokens.len(), "tokenizer.ggml.bos_token_id")?;
        validate_token_id(eos_id, tokens.len(), "tokenizer.ggml.eos_token_id")?;

        let add_bos = bool_meta(
            get_meta("tokenizer.ggml.add_bos_token"),
            "tokenizer.ggml.add_bos_token",
        )?;
        let add_eos = bool_meta(
            get_meta("tokenizer.ggml.add_eos_token"),
            "tokenizer.ggml.add_eos_token",
        )?;
        if add_bos && bos_id.is_none() {
            return Err("tokenizer.ggml.add_bos_token requires tokenizer.ggml.bos_token_id".into());
        }
        if add_eos && eos_id.is_none() {
            return Err("tokenizer.ggml.add_eos_token requires tokenizer.ggml.eos_token_id".into());
        }

        let mut special_tokens: Vec<SpecialToken> = tokens
            .iter()
            .zip(&token_types)
            .enumerate()
            .filter_map(|(id, (text, kind))| {
                matches!(
                    kind,
                    TokenType::Control | TokenType::Unknown | TokenType::UserDefined
                )
                .then(|| SpecialToken {
                    text: text.clone(),
                    id: id as u32,
                    kind: *kind,
                })
            })
            .collect();
        special_tokens.sort_by(|left, right| right.text.len().cmp(&left.text.len()));

        let semantic_tokens = QWEN_SEMANTIC_TOKENS
            .iter()
            .filter_map(|(literal, name)| {
                special_tokens
                    .iter()
                    .find(|token| token.text == *literal)
                    .map(|token| ((*name).to_string(), token.id))
            })
            .collect();

        let byte_fallback = match get_meta("tokenizer.ggml.byte_fallback") {
            Some(MetaValue::Bool(value)) => value,
            None => tokens.iter().any(|token| hex_byte(token).is_some()),
            Some(_) => return Err("Invalid tokenizer.ggml.byte_fallback: expected bool".into()),
        };

        Ok(Self {
            tokens,
            token_types,
            token_to_id,
            merge_ranks,
            byte_encoder,
            byte_decoder,
            pre,
            special_tokens,
            semantic_tokens,
            bos_id,
            eos_id,
            add_bos,
            add_eos,
            byte_fallback,
        })
    }

    fn encode_partitioned(&self, text: &str, parse_special: bool) -> Vec<u32> {
        let mut output = Vec::new();
        let mut remaining = text;

        while !remaining.is_empty() {
            let best = self
                .special_tokens
                .iter()
                .filter(|token| {
                    parse_special || !matches!(token.kind, TokenType::Control | TokenType::Unknown)
                })
                .filter_map(|token| {
                    remaining
                        .find(&token.text)
                        .map(|position| (position, token))
                })
                .min_by(|(left_pos, left), (right_pos, right)| {
                    left_pos
                        .cmp(right_pos)
                        .then_with(|| right.text.len().cmp(&left.text.len()))
                });

            let Some((position, token)) = best else {
                output.extend(self.encode_ordinary(remaining));
                break;
            };
            output.extend(self.encode_ordinary(&remaining[..position]));
            output.push(token.id);
            remaining = &remaining[position + token.text.len()..];
        }

        output
    }

    fn encode_ordinary(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::new();
        for range in scan_qwen_ranges(text, self.pre) {
            ids.extend(self.encode_bpe_segment(&text[range]));
        }
        ids
    }

    pub fn encode(&self, text: &str, options: EncodeOptions) -> Vec<u32> {
        let mut ids = self.encode_partitioned(text, options.parse_special);
        if options.add_special && self.add_bos {
            ids.insert(0, self.bos_id.expect("validated add_bos requires bos_id"));
        }
        if options.add_special && self.add_eos {
            ids.push(self.eos_id.expect("validated add_eos requires eos_id"));
        }
        ids
    }

    fn encode_bpe_segment(&self, text: &str) -> Vec<u32> {
        let bytes = text.as_bytes();
        let mut symbols: Vec<Symbol> = Vec::with_capacity(bytes.len());
        for &b in bytes {
            let token_str = if (b as usize) < self.byte_encoder.len() {
                self.byte_encoder[b as usize].clone()
            } else {
                format!("<0x{:02X}>", b)
            };
            symbols.push(Symbol {
                text: token_str,
                prev: 0,
                next: 0,
                n: 1,
            });
        }

        for i in 0..symbols.len() {
            symbols[i].prev = if i > 0 { i - 1 } else { usize::MAX };
            symbols[i].next = if i + 1 < symbols.len() {
                i + 1
            } else {
                usize::MAX
            };
        }

        loop {
            let mut best_rank = u32::MAX;
            let mut best_idx = 0;
            let mut found = false;

            let mut i = 0;
            while i < symbols.len() {
                if symbols[i].n == 0 {
                    i += 1;
                    continue;
                }
                let next = symbols[i].next;
                if next == usize::MAX || symbols[next].n == 0 {
                    i += 1;
                    continue;
                }

                if let Some(&rank) = self
                    .merge_ranks
                    .get(&(symbols[i].text.clone(), symbols[next].text.clone()))
                {
                    if rank < best_rank {
                        best_rank = rank;
                        best_idx = i;
                        found = true;
                    }
                }
                i += 1;
            }

            if !found {
                break;
            }

            let next = symbols[best_idx].next;
            symbols[best_idx].text = format!("{}{}", symbols[best_idx].text, symbols[next].text);
            symbols[best_idx].n += symbols[next].n;
            symbols[next].n = 0;
            symbols[best_idx].next = symbols[next].next;
            if symbols[best_idx].next != usize::MAX {
                let nn = symbols[best_idx].next;
                symbols[nn].prev = best_idx;
            }
        }

        let mut ids = Vec::new();
        for sym in &symbols {
            if sym.n == 0 {
                continue;
            }
            if let Some(&id) = self.token_to_id.get(&sym.text) {
                ids.push(id);
            } else if self.byte_fallback {
                for &b in sym.text.as_bytes() {
                    let hex_token = format!("<0x{:02X}>", b);
                    if let Some(&id) = self.token_to_id.get(&hex_token) {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    fn token_piece_bytes(&self, id: u32, render_special: bool) -> Vec<u8> {
        let Some(token) = self.tokens.get(id as usize) else {
            return Vec::new();
        };
        let kind = self.token_types[id as usize];
        if matches!(kind, TokenType::Control | TokenType::Unknown) && !render_special {
            return Vec::new();
        }
        if let Some(byte) = hex_byte(token) {
            return vec![byte];
        }

        let mut bytes = Vec::new();
        for value in token.chars() {
            if let Some(byte) = self.byte_decoder.get(&value) {
                bytes.push(*byte);
            } else {
                let mut encoded = [0u8; 4];
                bytes.extend_from_slice(value.encode_utf8(&mut encoded).as_bytes());
            }
        }
        bytes
    }

    pub fn decode_bytes(&self, ids: &[u32], render_special: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        for &id in ids {
            bytes.extend(self.token_piece_bytes(id, render_special));
        }
        bytes
    }

    pub fn decode(&self, ids: &[u32], render_special: bool) -> String {
        String::from_utf8_lossy(&self.decode_bytes(ids, render_special)).into_owned()
    }

    pub fn streaming_decoder(&self, render_special: bool) -> StreamingDecoder<'_> {
        StreamingDecoder {
            tokenizer: self,
            render_special,
            pending: Vec::new(),
        }
    }

    pub fn token_id(&self, literal: &str) -> Option<u32> {
        self.token_to_id.get(literal).copied()
    }

    pub fn special_token_id(&self, semantic_name: &str) -> Option<u32> {
        self.semantic_tokens.get(semantic_name).copied()
    }

    pub fn bos_id(&self) -> Option<u32> {
        self.bos_id
    }

    pub fn eos_id(&self) -> Option<u32> {
        self.eos_id
    }

    pub fn add_bos(&self) -> bool {
        self.add_bos
    }

    pub fn add_eos(&self) -> bool {
        self.add_eos
    }

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }

    pub fn token_str(&self, id: u32) -> &str {
        self.tokens
            .get(id as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

pub struct StreamingDecoder<'a> {
    tokenizer: &'a BPETokenizer,
    render_special: bool,
    pending: Vec<u8>,
}

impl StreamingDecoder<'_> {
    pub fn push(&mut self, id: u32) -> String {
        self.pending
            .extend(self.tokenizer.token_piece_bytes(id, self.render_special));
        let mut output = String::new();

        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(valid) => {
                    output.push_str(valid);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    output.push_str(
                        std::str::from_utf8(&self.pending[..valid])
                            .expect("valid_up_to prefix is UTF-8"),
                    );
                    self.pending.drain(..valid);
                    let Some(invalid_len) = error.error_len() else {
                        break;
                    };
                    output.push('\u{FFFD}');
                    self.pending.drain(..invalid_len.min(self.pending.len()));
                }
            }
        }
        output
    }

    pub fn finish(self) -> String {
        String::from_utf8_lossy(&self.pending).into_owned()
    }
}

fn hex_byte(piece: &str) -> Option<u8> {
    (piece.len() == 6 && piece.starts_with("<0x") && piece.ends_with('>'))
        .then(|| u8::from_str_radix(&piece[3..5], 16).ok())
        .flatten()
}

fn is_word_char(value: char, pre: PreTokenizer) -> bool {
    value.is_letter() || (pre == PreTokenizer::Qwen35 && value.is_mark())
}

fn is_number(value: char) -> bool {
    value.is_number()
}

fn is_punctuation_run_char(value: char, pre: PreTokenizer) -> bool {
    !value.is_whitespace() && !is_word_char(value, pre) && !is_number(value)
}

fn contraction_len(chars: &[char], pos: usize) -> Option<usize> {
    if chars.get(pos) != Some(&'\'') {
        return None;
    }
    let one = chars.get(pos + 1)?.to_ascii_lowercase();
    if matches!(one, 's' | 't' | 'm' | 'd') {
        return Some(2);
    }
    let two = chars.get(pos + 2)?.to_ascii_lowercase();
    if matches!((one, two), ('r', 'e') | ('v', 'e') | ('l', 'l')) {
        return Some(3);
    }
    None
}

fn scan_qwen_ranges(text: &str, pre: PreTokenizer) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let values: Vec<char> = chars.iter().map(|(_, value)| *value).collect();
    let byte_at = |index: usize| {
        chars
            .get(index)
            .map(|(byte, _)| *byte)
            .unwrap_or(text.len())
    };
    let mut ranges = Vec::new();
    let mut pos = 0usize;

    while pos < values.len() {
        let start = pos;
        let current = values[pos];

        if let Some(length) = contraction_len(&values, pos) {
            pos += length;
            ranges.push(byte_at(start)..byte_at(pos));
            continue;
        }

        if current != '\r' && current != '\n' && !is_number(current) {
            let starts_word = is_word_char(current, pre)
                || values
                    .get(pos + 1)
                    .copied()
                    .is_some_and(|value| is_word_char(value, pre));
            if starts_word {
                pos += 1;
                while values
                    .get(pos)
                    .copied()
                    .is_some_and(|value| is_word_char(value, pre))
                {
                    pos += 1;
                }
                ranges.push(byte_at(start)..byte_at(pos));
                continue;
            }
        }

        if is_number(current) {
            pos += 1;
            ranges.push(byte_at(start)..byte_at(pos));
            continue;
        }

        let punctuation_start = if current == ' ' { pos + 1 } else { pos };
        if values
            .get(punctuation_start)
            .copied()
            .is_some_and(|value| is_punctuation_run_char(value, pre))
        {
            pos = punctuation_start;
            while values
                .get(pos)
                .copied()
                .is_some_and(|value| is_punctuation_run_char(value, pre))
            {
                pos += 1;
            }
            while matches!(values.get(pos), Some('\r' | '\n')) {
                pos += 1;
            }
            ranges.push(byte_at(start)..byte_at(pos));
            continue;
        }

        let mut whitespace_count = 0usize;
        let mut last_newline_end = None;
        while let Some(value) = values.get(pos + whitespace_count) {
            if !value.is_whitespace() {
                break;
            }
            whitespace_count += 1;
            if matches!(value, '\r' | '\n') {
                last_newline_end = Some(pos + whitespace_count);
            }
        }
        if let Some(end) = last_newline_end {
            pos = end;
        } else if whitespace_count > 1 && pos + whitespace_count < values.len() {
            pos += whitespace_count - 1;
        } else if whitespace_count > 0 {
            pos += whitespace_count;
        } else {
            pos += 1;
        }
        ranges.push(byte_at(start)..byte_at(pos));
    }

    ranges
}

fn scan_qwen_words(text: &str, pre: PreTokenizer) -> Vec<&str> {
    scan_qwen_ranges(text, pre)
        .into_iter()
        .map(|range| &text[range])
        .collect()
}

struct Symbol {
    text: String,
    prev: usize,
    next: usize,
    n: usize,
}

fn build_byte_encoder() -> Vec<String> {
    let mut bs: Vec<u8> = Vec::new();
    for b in 33u8..=126 {
        bs.push(b);
    }
    for b in 161u8..=172 {
        bs.push(b);
    }
    for b in 174u8..=255 {
        bs.push(b);
    }
    let mut cs: Vec<u32> = bs.iter().map(|&b| b as u32).collect();
    let mut n = 0u32;
    for b in 0u8..=255u8 {
        if !bs.contains(&b) {
            bs.push(b);
            cs.push(256 + n);
            n += 1;
        }
    }
    let mut encoder = vec![String::new(); 256];
    for i in 0..bs.len() {
        if (bs[i] as usize) < 256 {
            encoder[bs[i] as usize] = char::from_u32(cs[i]).unwrap_or('?').to_string();
        }
    }
    encoder
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MetaValue, MetaValueType};
    use std::collections::HashMap;

    fn tokenizer_with_pre(pre: Option<&str>) -> Result<BPETokenizer, String> {
        let mut metadata: HashMap<String, MetaValue> = HashMap::from([
            (
                "tokenizer.ggml.model".to_string(),
                MetaValue::String("gpt2".into()),
            ),
            (
                "tokenizer.ggml.tokens".to_string(),
                MetaValue::Array(
                    MetaValueType::String,
                    ["a", "b", "Ġ", "Ġa", "Ġb"]
                        .into_iter()
                        .map(|value| MetaValue::String(value.into()))
                        .collect(),
                ),
            ),
            (
                "tokenizer.ggml.token_type".to_string(),
                MetaValue::Array(
                    MetaValueType::Uint32,
                    std::iter::repeat(MetaValue::Uint32(1)).take(5).collect(),
                ),
            ),
            (
                "tokenizer.ggml.merges".to_string(),
                MetaValue::Array(MetaValueType::String, vec![]),
            ),
        ]);
        if let Some(pre) = pre {
            metadata.insert("tokenizer.ggml.pre".into(), MetaValue::String(pre.into()));
        }
        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned())
    }

    #[test]
    fn rejects_missing_or_unknown_qwen_pre() {
        assert!(tokenizer_with_pre(None)
            .unwrap_err()
            .contains("tokenizer.ggml.pre"));
        assert!(tokenizer_with_pre(Some("default"))
            .unwrap_err()
            .contains("qwen2 or qwen35"));
    }

    #[test]
    fn qwen2_scanner_preserves_reference_boundaries() {
        assert_eq!(
            scan_qwen_words("hello   world", PreTokenizer::Qwen2),
            vec!["hello", "  ", " world"]
        );
        assert_eq!(scan_qwen_words("  a", PreTokenizer::Qwen2), vec![" ", " a"]);
        assert_eq!(
            scan_qwen_words("a  b", PreTokenizer::Qwen2),
            vec!["a", " ", " b"]
        );
    }

    #[test]
    fn qwen35_scanner_keeps_combining_marks_in_letter_runs() {
        assert_eq!(
            scan_qwen_words("e\u{301}", PreTokenizer::Qwen35),
            vec!["e\u{301}"]
        );
        assert_eq!(
            scan_qwen_words("re\u{301}sume\u{301}", PreTokenizer::Qwen35),
            vec!["re\u{301}sume\u{301}"]
        );
        assert_eq!(
            scan_qwen_words("Vieết Nam", PreTokenizer::Qwen35),
            vec!["Vieết", " Nam"]
        );
        assert_eq!(
            scan_qwen_words("e\u{301}", PreTokenizer::Qwen2),
            vec!["e", "\u{301}"]
        );
    }

    fn tokenizer_from_parts(
        tokens: &[&str],
        token_types: &[u32],
        bos_id: Option<u32>,
        eos_id: Option<u32>,
        add_bos: bool,
        add_eos: bool,
    ) -> Result<BPETokenizer, String> {
        let mut metadata: HashMap<String, MetaValue> = HashMap::from([
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
                    tokens
                        .iter()
                        .map(|value| MetaValue::String((*value).into()))
                        .collect(),
                ),
            ),
            (
                "tokenizer.ggml.token_type".into(),
                MetaValue::Array(
                    MetaValueType::Uint32,
                    token_types.iter().copied().map(MetaValue::Uint32).collect(),
                ),
            ),
            (
                "tokenizer.ggml.merges".into(),
                MetaValue::Array(MetaValueType::String, Vec::new()),
            ),
            (
                "tokenizer.ggml.add_bos_token".into(),
                MetaValue::Bool(add_bos),
            ),
            (
                "tokenizer.ggml.add_eos_token".into(),
                MetaValue::Bool(add_eos),
            ),
        ]);
        if let Some(id) = bos_id {
            metadata.insert("tokenizer.ggml.bos_token_id".into(), MetaValue::Uint32(id));
        }
        if let Some(id) = eos_id {
            metadata.insert("tokenizer.ggml.eos_token_id".into(), MetaValue::Uint32(id));
        }
        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned())
    }

    fn special_test_tokenizer() -> BPETokenizer {
        tokenizer_from_parts(
            &[
                "a",
                "b",
                "_",
                "<|im_start|>",
                "<tool_call>",
                "<|im_end|>",
                "<|image_pad|>",
                "<|vision_start|>",
                "<|vision_end|>",
                "<|audio_start|>",
                "<|audio_pad|>",
                "<|audio_end|>",
                "<asr_text>",
                "<|endoftext|>",
            ],
            &[1, 1, 1, 3, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3],
            None,
            Some(13),
            false,
            false,
        )
        .unwrap()
    }

    fn overlapping_special_test_tokenizer() -> BPETokenizer {
        tokenizer_from_parts(
            &["a", "b", "c", "d", "e", "f", "<tool>", "<tool_call>"],
            &[1, 1, 1, 1, 1, 1, 4, 4],
            None,
            None,
            false,
            false,
        )
        .unwrap()
    }

    fn bos_eos_test_tokenizer(add_bos: bool, add_eos: bool) -> BPETokenizer {
        tokenizer_from_parts(
            &["a", "b", "c", "d", "e", "f", "g", "h", "<s>", "</s>"],
            &[1, 1, 1, 1, 1, 1, 1, 1, 3, 3],
            Some(8),
            Some(9),
            add_bos,
            add_eos,
        )
        .unwrap()
    }

    fn tokenizer_with_token_types(types: &[u32]) -> Result<BPETokenizer, String> {
        tokenizer_from_parts(&["a", "b"], types, None, None, false, false)
    }

    fn normal_control_literal_test_tokenizer() -> BPETokenizer {
        tokenizer_from_parts(&["a", "<|im_start|>"], &[1, 1], None, None, false, false).unwrap()
    }

    #[test]
    fn parse_special_controls_and_user_defined_match_llama() {
        let tokenizer = special_test_tokenizer();
        assert_eq!(
            tokenizer.encode(
                "a<|im_start|>b<tool_call>",
                EncodeOptions {
                    add_special: false,
                    parse_special: true,
                },
            ),
            vec![0, 3, 1, 4],
        );
        assert_ne!(
            tokenizer.encode(
                "<|im_start|>",
                EncodeOptions {
                    add_special: false,
                    parse_special: false,
                },
            ),
            vec![3],
        );
        assert_eq!(
            tokenizer.encode(
                "<tool_call>",
                EncodeOptions {
                    add_special: false,
                    parse_special: false,
                },
            ),
            vec![4],
        );
    }

    #[test]
    fn special_partition_prefers_longest_literal() {
        let tokenizer = overlapping_special_test_tokenizer();
        assert_eq!(
            tokenizer.encode(
                "<tool_call>",
                EncodeOptions {
                    add_special: false,
                    parse_special: true,
                },
            ),
            vec![7],
        );
    }

    #[test]
    fn add_special_follows_flags_not_id_presence() {
        let tokenizer = bos_eos_test_tokenizer(false, true);
        assert_eq!(
            tokenizer.encode(
                "a",
                EncodeOptions {
                    add_special: true,
                    parse_special: true,
                },
            ),
            vec![0, 9],
        );
        assert_eq!(tokenizer.bos_id(), Some(8));
        assert_eq!(tokenizer.eos_id(), Some(9));
        assert!(!tokenizer.add_bos());
        assert!(tokenizer.add_eos());
    }

    #[test]
    fn malformed_token_type_is_rejected() {
        assert!(tokenizer_with_token_types(&[1])
            .unwrap_err()
            .contains("length"));
        assert!(tokenizer_with_token_types(&[1, 7])
            .unwrap_err()
            .contains("token type"));
    }

    #[test]
    fn semantic_literals_and_plain_names_are_distinct() {
        let tokenizer = special_test_tokenizer();
        let plain = EncodeOptions {
            add_special: false,
            parse_special: false,
        };
        let special = EncodeOptions {
            add_special: false,
            parse_special: true,
        };
        assert_eq!(tokenizer.encode("<|im_start|>", special), vec![3]);
        assert_ne!(tokenizer.encode("im_start", plain), vec![3]);
        assert_eq!(tokenizer.encode("<|endoftext|>", special), vec![13]);
        assert_eq!(tokenizer.encode("<tool_call>", plain), vec![4]);
        assert_eq!(tokenizer.special_token_id("im_start"), Some(3));
        assert_eq!(tokenizer.special_token_id("im_end"), Some(5));
        assert_eq!(tokenizer.special_token_id("image_pad"), Some(6));
        assert_eq!(tokenizer.special_token_id("vision_start"), Some(7));
        assert_eq!(tokenizer.special_token_id("vision_end"), Some(8));
        assert_eq!(tokenizer.special_token_id("endoftext"), Some(13));
    }

    #[test]
    fn asr_semantic_literals_resolve_to_token_ids() {
        let tokenizer = special_test_tokenizer();
        let audio_start_id = tokenizer.token_id("<|audio_start|>").unwrap();
        let audio_pad_id = tokenizer.token_id("<|audio_pad|>").unwrap();
        let audio_end_id = tokenizer.token_id("<|audio_end|>").unwrap();
        let asr_text_id = tokenizer.token_id("<asr_text>").unwrap();

        assert_eq!(
            tokenizer.special_token_id("audio_start"),
            Some(audio_start_id)
        );
        assert_eq!(tokenizer.special_token_id("audio_pad"), Some(audio_pad_id));
        assert_eq!(tokenizer.special_token_id("audio_end"), Some(audio_end_id));
        assert_eq!(tokenizer.special_token_id("asr_text"), Some(asr_text_id));
    }

    #[test]
    fn normal_control_looking_literal_has_no_chatml_semantic_name() {
        let tokenizer = normal_control_literal_test_tokenizer();
        assert_eq!(tokenizer.token_id("<|im_start|>"), Some(1));
        assert_eq!(tokenizer.special_token_id("im_start"), None);
    }

    fn byte_fallback_test_tokenizer(piece: &str) -> BPETokenizer {
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
                MetaValue::Array(MetaValueType::String, vec![MetaValue::String(piece.into())]),
            ),
            (
                "tokenizer.ggml.token_type".into(),
                MetaValue::Array(MetaValueType::Uint32, vec![MetaValue::Uint32(6)]),
            ),
            (
                "tokenizer.ggml.merges".into(),
                MetaValue::Array(MetaValueType::String, Vec::new()),
            ),
            ("tokenizer.ggml.byte_fallback".into(), MetaValue::Bool(true)),
        ]);
        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned()).unwrap()
    }

    fn tokenizer_from_env(name: &str) -> BPETokenizer {
        let path = std::env::var(name).unwrap();
        let loader = crate::model::GGUFLoader::from_file(&path).unwrap();
        BPETokenizer::from_gguf_metadata(|key| loader.metadata(key).cloned()).unwrap()
    }

    #[test]
    fn six_character_byte_fallback_decodes() {
        let tokenizer = byte_fallback_test_tokenizer("<0xF0>");
        assert_eq!(tokenizer.decode_bytes(&[0], false), vec![0xF0]);
    }

    #[test]
    fn render_special_controls_only_when_requested() {
        let tokenizer = special_test_tokenizer();
        let im_start = tokenizer.special_token_id("im_start").unwrap();
        assert_eq!(tokenizer.decode_bytes(&[im_start], false), b"");
        assert_eq!(tokenizer.decode_bytes(&[im_start], true), b"<|im_start|>");
        let tool = tokenizer.token_id("<tool_call>").unwrap();
        assert_eq!(tokenizer.decode_bytes(&[tool], false), b"<tool_call>");
    }

    #[test]
    #[ignore = "requires RMI_QWEN3_MODEL"]
    fn decode_bytes_joins_split_utf8_pieces() {
        let tokenizer = tokenizer_from_env("RMI_QWEN3_MODEL");
        assert_eq!(tokenizer.decode(&[9284, 104, 254], false), "🫠");
        assert_eq!(tokenizer.decode(&[124596, 252], false), "𝄞");
    }

    #[test]
    #[ignore = "requires RMI_QWEN3_MODEL"]
    fn streaming_decoder_buffers_incomplete_utf8() {
        let tokenizer = tokenizer_from_env("RMI_QWEN3_MODEL");
        let mut decoder = tokenizer.streaming_decoder(false);
        assert_eq!(decoder.push(9284), "");
        assert_eq!(decoder.push(104), "");
        assert_eq!(decoder.push(254), "🫠");
        assert_eq!(decoder.finish(), "");

        let mut decoder = tokenizer.streaming_decoder(false);
        assert_eq!(decoder.push(124596), "");
        assert_eq!(decoder.push(252), "𝄞");
        assert_eq!(decoder.finish(), "");
    }
}
