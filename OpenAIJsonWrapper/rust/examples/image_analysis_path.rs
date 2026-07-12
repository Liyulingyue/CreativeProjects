use openai_json_wrapper::{OpenAIClientBuilder, OpenAIJsonWrapper, Message, MessageContent, ContentPart, ChatOptions};
use serde_json::json;

fn main() {
    let target_structure = json!({
        "label": "string (图片分类标签)",
        "reason": "string (简短理由)"
    });

    let client = OpenAIClientBuilder::new("your-api-key")
        .base_url("https://api.minimaxi.com/v1")
        .build();

    let wrapper = OpenAIJsonWrapper::new(
        Box::new(client),
        "MiniMax-M3",
        Some(target_structure),
        None,
        Some("You are an expert at image classification."),
    );

    let messages = vec![Message {
        role: "user".to_string(),
        content: MessageContent::Array(vec![
            ContentPart::Text {
                part_type: "text".to_string(),
                text: "这张图片属于哪个类别？".to_string(),
            },
            ContentPart::ImagePath {
                part_type: "image_path".to_string(),
                image_path: "path/to/image.jpg".to_string(),
            },
        ]),
    }];

    println!("--- [image_path] 发送本地图片分析请求 ---\n");

    let result = wrapper.chat(messages, ChatOptions::default()).expect("chat failed");

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
