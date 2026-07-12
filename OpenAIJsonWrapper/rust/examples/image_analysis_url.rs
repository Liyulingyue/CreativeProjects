use openai_json_wrapper::{OpenAIClientBuilder, OpenAIJsonWrapper, Message, MessageContent, ContentPart, ImageUrlValue, ChatOptions};
use serde_json::json;

fn main() {
    let target_structure = json!({
        "score": "int, 0-100, 代表照片质量评分",
        "style": "str, 照片风格描述",
        "caption": "str, 用中文写一句话，不超过30字",
        "main_objects": "list[str], 至少2个主要物体",
        "blurry": "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰'三选一",
        "comments": "str, 对照片的详细评价，至少50字",
        "recommendations": "str, 对拍摄者的改进建议，至少30字"
    });

    let client = OpenAIClientBuilder::new("your-api-key")
        .base_url("https://api.minimaxi.com/v1")
        .build();

    let wrapper = OpenAIJsonWrapper::new(
        Box::new(client),
        "MiniMax-M3",
        Some(target_structure),
        None,
        Some("You are a professional travel photo analyst."),
    );

    let messages = vec![Message {
        role: "user".to_string(),
        content: MessageContent::Array(vec![
            ContentPart::Text {
                part_type: "text".to_string(),
                text: "请仔细观察这张图片，按指定 JSON 结构输出。".to_string(),
            },
            ContentPart::ImageUrl {
                part_type: "image_url".to_string(),
                image_url: ImageUrlValue::String("https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/Cat03.jpg/1200px-Cat03.jpg".to_string()),
            },
        ]),
    }];

    println!("--- [image_url] 发送多模态请求 ---\n");

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
