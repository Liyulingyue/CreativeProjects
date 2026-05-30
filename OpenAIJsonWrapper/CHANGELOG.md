# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-05-30

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
