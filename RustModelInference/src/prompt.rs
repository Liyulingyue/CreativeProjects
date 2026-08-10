use crate::{BPETokenizer, EncodeOptions};

pub struct QwenMessage<'a> {
    pub role: &'a str,
    pub content: &'a str,
}

const PLAIN_TEXT: EncodeOptions = EncodeOptions {
    add_special: false,
    parse_special: false,
};

fn required_control(tokenizer: &BPETokenizer, name: &str, literal: &str) -> Result<u32, String> {
    tokenizer
        .special_token_id(name)
        .ok_or_else(|| format!("Required ChatML token missing: {literal}"))
}

pub fn build_qwen_chat_prompt(
    tokenizer: &BPETokenizer,
    messages: &[QwenMessage<'_>],
) -> Result<Vec<u32>, String> {
    let mut output = Vec::new();
    for message in messages {
        let content = tokenizer.encode(message.content, PLAIN_TEXT);
        append_qwen_message_tokens(&mut output, tokenizer, message.role, &content)?;
    }
    append_qwen_assistant_prefix(&mut output, tokenizer)?;
    Ok(output)
}

pub fn append_qwen_message_tokens(
    out: &mut Vec<u32>,
    tokenizer: &BPETokenizer,
    role: &str,
    content_tokens: &[u32],
) -> Result<(), String> {
    let im_start = required_control(tokenizer, "im_start", "<|im_start|>")?;
    let im_end = required_control(tokenizer, "im_end", "<|im_end|>")?;
    out.push(im_start);
    out.extend(tokenizer.encode(&format!("{role}\n"), PLAIN_TEXT));
    out.extend_from_slice(content_tokens);
    out.push(im_end);
    out.extend(tokenizer.encode("\n", PLAIN_TEXT));
    Ok(())
}

pub fn append_qwen_assistant_prefix(
    out: &mut Vec<u32>,
    tokenizer: &BPETokenizer,
) -> Result<(), String> {
    out.push(required_control(tokenizer, "im_start", "<|im_start|>")?);
    out.extend(tokenizer.encode("assistant\n", PLAIN_TEXT));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetaValue, MetaValueType};
    use std::collections::HashMap;

    fn prompt_tokenizer(include_im_end: bool) -> BPETokenizer {
        let mut tokens = vec!["u", "s", "e", "r", "Ċ", "H", "i", "<|im_start|>"];
        let mut types = vec![1u32; tokens.len()];
        types[7] = 3;
        if include_im_end {
            tokens.push("<|im_end|>");
            types.push(3);
        }
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
                    tokens
                        .into_iter()
                        .map(|value| MetaValue::String(value.into()))
                        .collect(),
                ),
            ),
            (
                "tokenizer.ggml.token_type".into(),
                MetaValue::Array(
                    MetaValueType::Uint32,
                    types.into_iter().map(MetaValue::Uint32).collect(),
                ),
            ),
            (
                "tokenizer.ggml.merges".into(),
                MetaValue::Array(MetaValueType::String, vec![]),
            ),
        ]);
        BPETokenizer::from_gguf_metadata(|key| metadata.get(key).cloned()).unwrap()
    }

    #[test]
    fn token_content_uses_the_same_message_envelope() {
        let tokenizer = prompt_tokenizer(true);
        let mut output = Vec::new();
        append_qwen_message_tokens(&mut output, &tokenizer, "user", &[5, 6]).unwrap();
        assert_eq!(output, vec![7, 0, 1, 2, 3, 4, 5, 6, 8, 4]);
    }

    #[test]
    fn missing_chatml_literal_is_an_error() {
        let tokenizer = prompt_tokenizer(false);
        let error = build_qwen_chat_prompt(
            &tokenizer,
            &[QwenMessage {
                role: "user",
                content: "Hi",
            }],
        )
        .unwrap_err();
        assert!(error.contains("<|im_end|>"), "{error}");
    }
}
