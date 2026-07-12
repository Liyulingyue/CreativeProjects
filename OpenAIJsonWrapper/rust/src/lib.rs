mod client;
mod types;
mod wrapper;

pub use client::{OpenAIClientBuilder, ReqwestOpenAIClient};
pub use types::*;
pub use wrapper::OpenAIJsonWrapper;

use regex::Regex;
use serde_json::Value;
use std::path::Path;
use base64::{Engine as _, engine::general_purpose};

pub struct ChatOptions {
    pub target_structure: Option<Value>,
    pub requirements: Option<String>,
    pub extra_requirements: Option<String>,
    pub background: Option<String>,
    pub model: Option<String>,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            target_structure: None,
            requirements: None,
            extra_requirements: None,
            background: None,
            model: None,
        }
    }
}

fn build_system_prompt(
    target_structure: &Value,
    requirements: Option<&[String]>,
    background: Option<&str>,
) -> String {
    let structure_str = serde_json::to_string_pretty(target_structure).unwrap_or_default();
    let mut prompt = "You are a helpful assistant that MUST output your response in a specific JSON format.\n".to_string();

    if let Some(bg) = background {
        prompt.push_str(&format!("\nBackground Information:\n{}\n", bg));
    }

    prompt.push_str(&format!("\nThe required JSON structure is:\n{}\n\n", structure_str));

    if let Some(reqs) = requirements {
        if !reqs.is_empty() {
            let req_text = reqs.iter().map(|r| format!("- {}", r)).collect::<Vec<_>>().join("\n");
            prompt.push_str(&format!("Specific Requirements:\n{}\n\n", req_text));
        }
    }

    prompt.push_str(&format!(
        "Rules:\n\
        1. Your final JSON data MUST be wrapped between '{}' and '{}' markdown blocks.\n\
        2. Everything before the code block is considered your reasoning or conversational text.\n\
        3. Ensure the JSON inside the block is valid and matches the requested structure strictly.\n",
        types::TOOL_MARKER_START,
        types::TOOL_MARKER_END
    ));

    prompt
}

fn parse_content(text: &str) -> (String, Option<Value>, Option<String>) {
    if text.is_empty() {
        return (String::new(), None, Some("Empty content".to_string()));
    }

    let think_end_marker = "</think>";
    let text = if let Some(pos) = text.find(think_end_marker) {
        text[pos + think_end_marker.len()..].trim()
    } else {
        text
    };

    let md_pattern = Regex::new(r"```json\s*([\s\S]*?)\s*```").unwrap();
    if let Some(captures) = md_pattern.captures(text) {
        let inner = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        let reasoning = text[..captures.get(0).map(|m| m.start()).unwrap_or(0)].trim().to_string();

        if let Ok(parsed) = serde_json::from_str::<Value>(inner) {
            return (reasoning, Some(parsed), None);
        }

        let cleaned_pattern = Regex::new(r",(\s*[}\]])").unwrap();
        let cleaned = cleaned_pattern.replace_all(inner, "$1");
        if let Ok(parsed) = serde_json::from_str::<Value>(&cleaned) {
            return (reasoning, Some(parsed), None);
        }

        return (reasoning, None, Some(format!("JSON parse error in markdown block")));
    }

    for (opener, closer) in [("[", "]"), ("{", "}")] {
        if let Some(idx) = text.rfind(opener) {
            let mut cand = text[idx..].trim().to_string();
            if let Some(last_closer) = cand.rfind(closer) {
                cand.truncate(last_closer + 1);
            }
            if let Ok(parsed) = serde_json::from_str::<Value>(&cand) {
                let reasoning = text[..idx].trim().to_string();
                return (reasoning, Some(parsed), None);
            }
        }
    }

    (text.to_string(), None, Some("No JSON structure found".to_string()))
}

fn encode_image(image_source: &str) -> Result<String, String> {
    if image_source.starts_with("http://") || image_source.starts_with("https://") {
        return Ok(image_source.to_string());
    }

    let path = Path::new(image_source);
    if !path.exists() {
        return Err(format!("Image not found: {}", image_source));
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "jpeg".to_string());

    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/jpeg",
    };

    let data = std::fs::read(path).map_err(|e| format!("Failed to read image: {}", e))?;
    let encoded = general_purpose::STANDARD.encode(&data);

    Ok(format!("data:{};base64,{}", mime, encoded))
}

fn normalize_content_part(part: &ContentPart) -> Result<ContentPart, String> {
    match part {
        ContentPart::ImagePath { image_path, .. } => {
            let url = encode_image(image_path)?;
            Ok(ContentPart::ImageUrl {
                part_type: "image_url".to_string(),
                image_url: ImageUrlValue::Object {
                    url,
                    detail: None,
                },
            })
        }
        ContentPart::ImageUrl { image_url, .. } => {
            let url_str = match image_url {
                ImageUrlValue::String(s) => s.clone(),
                ImageUrlValue::Object { url, .. } => url.clone(),
            };

            if url_str.starts_with("http://") || url_str.starts_with("https://") || url_str.starts_with("data:") {
                Ok(ContentPart::ImageUrl {
                    part_type: "image_url".to_string(),
                    image_url: ImageUrlValue::Object {
                        url: url_str,
                        detail: None,
                    },
                })
            } else {
                let encoded = encode_image(&url_str)?;
                Ok(ContentPart::ImageUrl {
                    part_type: "image_url".to_string(),
                    image_url: ImageUrlValue::Object {
                        url: encoded,
                        detail: None,
                    },
                })
            }
        }
        _ => Ok(part.clone()),
    }
}

fn normalize_message(message: &Message) -> Result<Message, String> {
    let content = match &message.content {
        MessageContent::Array(parts) => {
            let normalized: Result<Vec<ContentPart>, String> = parts
                .iter()
                .map(normalize_content_part)
                .collect();
            MessageContent::Array(normalized?)
        }
        MessageContent::String(_) => message.content.clone(),
    };

    Ok(Message {
        role: message.role.clone(),
        content,
    })
}
