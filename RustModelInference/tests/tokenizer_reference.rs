use rust_model_inference::{
    build_qwen_chat_prompt, BPETokenizer, GGUFLoader, QwenMessage,
};

#[test]
#[ignore = "requires RMI_QWEN3_MODEL"]
fn qwen_chat_prompt_matches_reference_ids() {
    let model = std::env::var("RMI_QWEN3_MODEL").unwrap();
    let loader = GGUFLoader::from_file(&model).unwrap();
    let tokenizer =
        BPETokenizer::from_gguf_metadata(|key| loader.metadata(key).cloned()).unwrap();
    assert_eq!(
        build_qwen_chat_prompt(
            &tokenizer,
            &[QwenMessage {
                role: "user",
                content: "Hello",
            }],
        )
        .unwrap(),
        vec![
            151644, 872, 198, 9707, 151645, 198, 151644, 77091, 198,
        ],
    );
}

#[test]
#[ignore = "requires RMI_QWEN3_MODEL"]
fn qwen_system_user_assistant_prompt_matches_reference_ids() {
    let model = std::env::var("RMI_QWEN3_MODEL").unwrap();
    let loader = GGUFLoader::from_file(&model).unwrap();
    let tokenizer =
        BPETokenizer::from_gguf_metadata(|key| loader.metadata(key).cloned()).unwrap();
    assert_eq!(
        build_qwen_chat_prompt(
            &tokenizer,
            &[
                QwenMessage {
                    role: "system",
                    content: "You are concise.",
                },
                QwenMessage {
                    role: "user",
                    content: "Hello",
                },
                QwenMessage {
                    role: "assistant",
                    content: "Acknowledged.",
                },
            ],
        )
        .unwrap(),
        vec![
            151644, 8948, 198, 2610, 525, 63594, 13, 151645, 198, 151644, 872, 198, 9707,
            151645, 198, 151644, 77091, 198, 90236, 3556, 13, 151645, 198, 151644, 77091,
            198,
        ],
    );
}
