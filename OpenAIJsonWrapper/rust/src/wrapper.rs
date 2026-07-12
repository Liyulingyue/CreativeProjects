use crate::{build_system_prompt, parse_content, normalize_message, ChatOptions, Message, MessageContent};
use crate::types::{ChatCompletionRequest, OpenAIClient, ChatResult};
use serde_json::Value;

pub struct OpenAIJsonWrapper {
    client: Box<dyn OpenAIClient>,
    model: String,
    target_structure: Option<Value>,
    requirements: Vec<String>,
    background: Option<String>,
}

impl OpenAIJsonWrapper {
    pub fn new(
        client: Box<dyn OpenAIClient>,
        model: &str,
        target_structure: Option<Value>,
        requirements: Option<Vec<&str>>,
        background: Option<&str>,
    ) -> Self {
        Self {
            client,
            model: model.to_string(),
            target_structure,
            requirements: requirements.unwrap_or_default().iter().map(|s| s.to_string()).collect(),
            background: background.map(|s| s.to_string()),
        }
    }

    pub fn chat(&self, messages: Vec<Message>, options: ChatOptions) -> Result<ChatResult, String> {
        let target = options.target_structure.as_ref().or(self.target_structure.as_ref());
        let target = target.ok_or("target_structure must be provided either in __init__ or in chat()")?;

        let mut reqs = self.requirements.clone();
        if let Some(r) = options.requirements {
            reqs.push(r);
        }
        if let Some(r) = options.extra_requirements {
            reqs.push(r);
        }
        let reqs_for_prompt = if reqs.is_empty() { None } else { Some(&reqs[..]) };

        let bg = options.background.as_deref().or(self.background.as_deref());
        let model = options.model.as_deref().unwrap_or(&self.model);

        let system_prompt = build_system_prompt(target, reqs_for_prompt, bg);

        let mut new_messages: Vec<Message> = Vec::new();
        let mut has_system = false;

        for m in &messages {
            if m.role == "system" {
                let sys_content = match &m.content {
                    MessageContent::String(s) => format!("{}\n\n{}", system_prompt, s),
                    MessageContent::Array(_) => format!("{}\n\n{}", system_prompt, serde_json::to_string(&m.content).unwrap_or_default()),
                };
                new_messages.push(Message {
                    role: "system".to_string(),
                    content: MessageContent::String(sys_content),
                });
                has_system = true;
            } else {
                new_messages.push(normalize_message(m)?);
            }
        }

        if !has_system {
            new_messages.insert(0, Message {
                role: "system".to_string(),
                content: MessageContent::String(system_prompt),
            });
        }

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: new_messages,
        };

        let response = self.client.chat_completions_create(request)?;

        let choice = response.choices.first()
            .ok_or("No choices in response")?;
        let content = choice.message.content.as_deref().unwrap_or("");

        let (reasoning, data, error) = parse_content(content);

        Ok(ChatResult {
            reasoning,
            data,
            error,
            raw_content: content.to_string(),
            response_id: Some(response.id),
        })
    }
}
