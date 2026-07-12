use openai_json_wrapper::{OpenAIClientBuilder, OpenAIJsonWrapper, Message, MessageContent, ChatOptions};
use serde_json::json;
use std::env;

fn main() {
    for path in [".env", "../.env", "../../.env"] {
        if dotenvy::from_filename(path).is_ok() {
            break;
        }
    }

    let api_key = env::var("OPENAI_API_KEY").unwrap_or_else(|_| {
        println!("警告: 未检测到 OPENAI_API_KEY，请检查 .env 文件或设置环境变量");
        "your-api-key-here".to_string()
    });
    let base_url = env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.minimaxi.com/v1".to_string());
    let model_name = env::var("OPENAI_MODEL_NAME").unwrap_or_else(|_| "MiniMax-M3".to_string());

    if api_key == "your-api-key-here" {
        println!("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。");
    }

    let target_structure = json!({
        "analysis": {
            "sentiment": "string (Positive/Negative/Neutral)",
            "key_entities": ["string"],
            "confidence_score": "float (0-1)"
        },
        "response_suggestion": "string"
    });

    let client = OpenAIClientBuilder::new(&api_key)
        .base_url(&base_url)
        .build();

    let wrapper = OpenAIJsonWrapper::new(
        Box::new(client),
        &model_name,
        Some(target_structure.clone()),
        None,
        Some("You are a professional product review analyst."),
    );

    let messages = vec![Message {
        role: "user".to_string(),
        content: MessageContent::String("GitHub Copilot 是一款非常棒的 AI 编程助手，它能极大地提高代码生产力，虽然偶尔会有小瑕疵，但整体瑕不掩瑜。".to_string()),
    }];

    let options = ChatOptions {
        requirements: Some("情感分析必须细化到分值".to_string()),
        extra_requirements: Some("response_suggestion 必须是中文".to_string()),
        background: Some("你是一个专业级的产品评论分析专家，擅长从用户反馈中提取结构化洞察。".to_string()),
        ..Default::default()
    };

    println!("--- 发送请求 ---\n");

    let result = wrapper.chat(messages, options).expect("chat failed");

    println!("\n--- 解析结果 ---");
    if let Some(error) = result.error {
        println!("解析失败!");
        println!("错误信息: {}", error);
        println!("原始响应:\n{}", result.raw_content);
    } else {
        println!("成功解析数据:");
        println!("{}", serde_json::to_string_pretty(&result.data).unwrap());
        println!("\n思维链/推理过程:");
        println!("{}", result.reasoning);
    }
}
