use std::collections::HashMap;

use crate::model::MetaValue;

pub struct BPETokenizer {
    tokens: Vec<String>,
    token_to_id: HashMap<String, u32>,
    merge_ranks: HashMap<(String, String), u32>,
    byte_encoder: Vec<String>,
    byte_decoder: HashMap<char, u8>,
    special_tokens: HashMap<String, u32>,
    bos_id: u32,
    eos_id: u32,
    byte_fallback: bool,
}

impl BPETokenizer {
    pub fn from_gguf_metadata(
        get_meta: impl Fn(&str) -> Option<MetaValue>,
    ) -> Result<Self, String> {
        let tokens = match get_meta("tokenizer.ggml.tokens") {
            Some(MetaValue::Array(_, vals)) => {
                vals.iter().map(|v| match v {
                    MetaValue::String(s) => s.clone(),
                    _ => String::new(),
                }).collect::<Vec<_>>()
            }
            _ => return Err("Missing tokenizer.ggml.tokens".into()),
        };

        let mut token_to_id = HashMap::new();
        for (i, t) in tokens.iter().enumerate() {
            token_to_id.insert(t.clone(), i as u32);
        }

        let merge_ranks = match get_meta("tokenizer.ggml.merges") {
            Some(MetaValue::Array(_, vals)) => {
                let mut ranks = HashMap::new();
                for (i, v) in vals.iter().enumerate() {
                    if let MetaValue::String(s) = v {
                        let parts: Vec<&str> = s.split(' ').collect();
                        if parts.len() == 2 {
                            ranks.insert((parts[0].to_string(), parts[1].to_string()), i as u32);
                        }
                    }
                }
                ranks
            }
            _ => HashMap::new(),
        };

        let byte_encoder = build_byte_encoder();
        let mut byte_decoder = HashMap::new();
        for (b, s) in byte_encoder.iter().enumerate() {
            if !s.is_empty() {
                if let Some(ch) = s.chars().next() {
                    byte_decoder.insert(ch, b as u8);
                }
            }
        }

        let bos_id = get_meta("tokenizer.ggml.bos_token_id")
            .and_then(|v| v.to_u64())
            .unwrap_or(0) as u32;
        let eos_id = get_meta("tokenizer.ggml.eos_token_id")
            .and_then(|v| v.to_u64())
            .unwrap_or(0) as u32;

        let byte_fallback = tokens.iter().any(|t| t.starts_with("<0x") && t.ends_with('>'));

        Ok(Self { tokens, token_to_id, merge_ranks, byte_encoder, byte_decoder, special_tokens: HashMap::new(), bos_id, eos_id, byte_fallback })
    }

    pub fn encode(&self, text: &str) -> Vec<u32> {
        if !self.special_tokens.is_empty() {
            let mut result = Vec::new();
            let mut remaining = text;
            while !remaining.is_empty() {
                let mut best_pos = usize::MAX;
                let mut best_len = 0;
                let mut best_id = 0u32;
                for (token_str, &id) in &self.special_tokens {
                    if let Some(pos) = remaining.find(token_str.as_str()) {
                        if pos < best_pos || (pos == best_pos && token_str.len() > best_len) {
                            best_pos = pos;
                            best_len = token_str.len();
                            best_id = id;
                        }
                    }
                }
                if best_pos == usize::MAX {
                    result.extend_from_slice(&self.encode_bpe(remaining));
                    break;
                }
                if best_pos > 0 {
                    result.extend_from_slice(&self.encode_bpe(&remaining[..best_pos]));
                }
                result.push(best_id);
                remaining = &remaining[best_pos + best_len..];
            }
            return result;
        }
        self.encode_bpe(text)
    }

    fn encode_bpe(&self, text: &str) -> Vec<u32> {
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
            symbols[i].next = if i + 1 < symbols.len() { i + 1 } else { usize::MAX };
        }

        loop {
            let mut best_rank = u32::MAX;
            let mut best_idx = 0;
            let mut found = false;

            let mut i = 0;
            while i < symbols.len() {
                if symbols[i].n == 0 { i += 1; continue; }
                let next = symbols[i].next;
                if next == usize::MAX || symbols[next].n == 0 { i += 1; continue; }

                if let Some(&rank) = self.merge_ranks.get(&(symbols[i].text.clone(), symbols[next].text.clone())) {
                    if rank < best_rank {
                        best_rank = rank;
                        best_idx = i;
                        found = true;
                    }
                }
                i += 1;
            }

            if !found { break; }

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
            if sym.n == 0 { continue; }
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

    pub fn decode(&self, ids: &[u32]) -> String {
        let mut bytes = Vec::new();
        for &id in ids {
            if (id as usize) < self.tokens.len() {
                let token = &self.tokens[id as usize];
                if token.starts_with("<0x") && token.ends_with('>') && token.len() == 5 {
                    if let Ok(b) = u8::from_str_radix(&token[3..5], 16) {
                        bytes.push(b);
                        continue;
                    }
                }
                for ch in token.chars() {
                    if let Some(&b) = self.byte_decoder.get(&ch) {
                        bytes.push(b);
                    } else {
                        let mut buf = [0u8; 4];
                        let s = ch.encode_utf8(&mut buf);
                        bytes.extend_from_slice(s.as_bytes());
                    }
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn bos_id(&self) -> u32 { self.bos_id }
    pub fn eos_id(&self) -> u32 { self.eos_id }
    pub fn vocab_size(&self) -> usize { self.tokens.len() }
    pub fn token_str(&self, id: u32) -> &str {
        self.tokens.get(id as usize).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn set_special_tokens(&mut self, specials: HashMap<String, u32>) {
        self.special_tokens = specials;
    }

    pub fn special_token_id(&self, name: &str) -> Option<u32> {
        self.special_tokens.get(name).copied()
    }
}

struct Symbol {
    text: String,
    prev: usize,
    next: usize,
    n: usize,
}

fn build_byte_encoder() -> Vec<String> {
    let mut bs: Vec<u8> = Vec::new();
    for b in 33u8..=126 { bs.push(b); }
    for b in 161u8..=172 { bs.push(b); }
    for b in 174u8..=255 { bs.push(b); }
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
