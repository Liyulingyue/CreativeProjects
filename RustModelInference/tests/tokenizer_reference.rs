use rust_model_inference::{
    build_qwen_chat_prompt, BPETokenizer, EncodeOptions, GGUFLoader, QwenMessage,
};
use std::fs;
use std::path::{Path, PathBuf};

const CASE_SEPARATOR: &str = "\n__ggml_vocab_test__\n";
const PLAIN: EncodeOptions = EncodeOptions {
    add_special: false,
    parse_special: false,
};

fn llama_models_dir() -> PathBuf {
    Path::new(&std::env::var("LLAMA_CPP_DIR").unwrap()).join("models")
}

fn parse_expected(line: &str) -> Vec<u32> {
    line.split_whitespace()
        .map(|value| value.parse::<u32>().unwrap())
        .collect()
}

fn check_vocab_fixture(name: &str) {
    let base = llama_models_dir().join(format!("ggml-vocab-{name}.gguf"));
    let loader = GGUFLoader::from_file(base.to_str().unwrap()).unwrap();
    let tokenizer =
        BPETokenizer::from_gguf_metadata(|key| loader.metadata(key).cloned()).unwrap();
    let inputs = fs::read_to_string(base.with_extension("gguf.inp")).unwrap();
    let outputs = fs::read_to_string(base.with_extension("gguf.out")).unwrap();
    let cases: Vec<&str> = inputs
        .strip_suffix(CASE_SEPARATOR)
        .unwrap_or(&inputs)
        .split(CASE_SEPARATOR)
        .collect();
    let expected: Vec<Vec<u32>> = outputs.lines().map(parse_expected).collect();
    assert_eq!(cases.len(), expected.len(), "fixture {name} is malformed");

    for (index, (text, ids)) in cases.iter().zip(&expected).enumerate() {
        assert_eq!(
            tokenizer.encode(text, PLAIN),
            *ids,
            "{name} fixture case {index}: {text:?}",
        );
    }
}

#[test]
#[ignore = "requires LLAMA_CPP_DIR"]
fn qwen2_vocab_fixture_matches_pinned_llama_cpp() {
    check_vocab_fixture("qwen2");
}

#[test]
#[ignore = "requires LLAMA_CPP_DIR"]
fn qwen35_vocab_fixture_matches_pinned_llama_cpp() {
    check_vocab_fixture("qwen35");
}

#[test]
#[ignore = "requires LLAMA_CPP_DIR"]
fn fixed_qwen_literal_ids_match_pinned_llama_cpp() {
    let qwen2_path = llama_models_dir().join("ggml-vocab-qwen2.gguf");
    let qwen2_loader = GGUFLoader::from_file(qwen2_path.to_str().unwrap()).unwrap();
    let qwen2 = BPETokenizer::from_gguf_metadata(|key| qwen2_loader.metadata(key).cloned())
        .unwrap();
    assert_eq!(qwen2.encode("hello   world", PLAIN), vec![14990, 256, 1879]);
    assert_eq!(qwen2.encode("  a", PLAIN), vec![220, 264]);
    assert_eq!(qwen2.encode("a  b", PLAIN), vec![64, 220, 293]);

    let qwen35_path = llama_models_dir().join("ggml-vocab-qwen35.gguf");
    let qwen35_loader = GGUFLoader::from_file(qwen35_path.to_str().unwrap()).unwrap();
    let qwen35 = BPETokenizer::from_gguf_metadata(|key| qwen35_loader.metadata(key).cloned())
        .unwrap();
    assert_eq!(qwen35.encode("e\u{301}", PLAIN), vec![68, 53839]);
    assert_eq!(
        qwen35.encode("re\u{301}sume\u{301}", PLAIN),
        vec![265, 53839, 31323, 53839]
    );
    assert_eq!(
        qwen35.encode("Vieết Nam", PLAIN),
        vec![53, 645, 51580, 29974]
    );
}

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
