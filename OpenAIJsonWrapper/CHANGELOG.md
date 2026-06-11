# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- **`chat()` 多模态支持**: `chat()` 现在接受 OpenAI 风格的多模态消息，即 message 的 `content` 可以是 `list[part]`，支持 `text` / `image_url` (dict 或 URL 字符串) / 新增的 `image_path` 类型；本地图片路径会自动 base64 编码、URL 直接透传。
- 内部辅助方法：`_normalize_message` / `_normalize_content_part`，统一处理单条消息与每个 part 的标准化。
- 离线单元测试 `tests/test_chat_multimodal.py`：使用 Mock 客户端验证纯文本/多模态图片输入的行为。
- 真实客户端示例 `tests/test_real_client_img.py`：演示 `chat()` 接入 `image_path` / `image_url` 两种 part。

### Removed
- **`vision()` 方法**: 单图快捷接口已移除，统一通过 `chat()` + 多模态 part 调用，避免 API 表面膨胀。

## [0.2.0] - 2026-05-30

### Added
- **`vision()` 方法**: 新增多模态图片分析支持，自动处理本地图片路径或 URL，构造多模态消息格式，用 vision 模型返回结构化 JSON 结果。
- **`_encode_image()`**: 内置方法，支持本地图片 base64 编码和 MIME 类型自动识别（jpg/png/gif/webp/bmp）。

### Changed
- 描述更新：`description` 改为"支持多模态图片输入"

## [0.1.0] - 2025-01-01

### Added
- 初始版本，支持文本聊天的 JSON 结构化输出
- 自动 System Prompt 注入
- ` ```json ` 块解析与容错
- `background` / `requirements` / `extra_requirements` 配置
- DeepSeek `</think>` 思维链兼容
