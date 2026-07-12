# OpenAIJsonWrapper for Rust

轻量级 OpenAI 响应解析封装，强制模型输出 JSON 并自动解析。

## 安装

```toml
[dependencies]
openaijsonwrapper = "0.2.0"
```

## 快速开始

```rust
use openaijsonwrapper::{OpenAIJsonWrapper, OpenAIClientBuilder, Message, MessageContent};
use serde_json::json;

let client = OpenAIClientBuilder::new("your-api-key")
    .base_url("https://api.openai.com/v1")
    .build();

let target_structure = json!({
    "sentiment": "string (Positive/Negative/Neutral)",
    "confidence_score": "float (0-1)"
});

let wrapper = OpenAIJsonWrapper::new(
    Box::new(client),
    "gpt-4",
    Some(target_structure),
    None,
    None,
);

let messages = vec![Message {
    role: "user".to_string(),
    content: MessageContent::String("I love this product!".to_string()),
}];

let result = wrapper.chat(messages, Default::default()).unwrap();

println!("{:?}", result.data);
```

## 特性

- **自动提示词注入**：JSON 结构定义自动拼入 System Prompt
- **DeepSeek 兼容**：自动处理 `</think>` 标记
- **健壮解析**：正则提取 ` ```json ` 块，支持容错
- **多模态支持**：支持本地图片路径和 URL

## 依赖

- `reqwest` - HTTP 客户端
- `serde_json` - JSON 序列化
- `base64` - 图片 base64 编码
- `regex` - 正则表达式
- `thiserror` - 错误处理

## License

MIT
