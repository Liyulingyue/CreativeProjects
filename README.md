# 🚀 CreativeProjects

<div align="center">

**Liyulingyue 的创意项目合集**

这是一个汇集各类创新项目的仓库，涵盖 LLM 集成、多模态模型、AI 工具和性能测试等多个领域。

</div>

---

## 📑 快速导航

| 分类 | 项目 | 描述 |
|------|------|------|
| 🤖 LLM 服务 | [ErnieToolCallsProxy](#ernietoolcallsproxy) | LLM 代理服务 |
| 🤖 LLM 服务 | [Llama-cpp-ernie-tutorial](#llama-cpp-ernie-tutorial) | ERNIE GGUF 部署 |
| 🤖 LLM 服务 | [OpenCodeServiceswithRknnLllm](#opencodewithrknn) | RK3588S 编程助手 |
| 🎯 搜索排序 | [OpenAIRerankServer](#openaireranksserver) | 文档重排服务 |
| 📊 性能测试 | [OpenAIConcurrencyTesting](#openaiconcurrencytesting) | API 并发测试 |
| 🔤 文本识别 | [PaddleOCR-VL-CPU](#paddleocr-vl-cpu) | CPU 文本理解 |
| 🔤 文本识别 | [PaddleOCR-VL-GGUF](#paddleocr-vl-gguf) | GGUF 文本理解 |
| � 文件管理 | [FileStation](#filestation) | 极简、高性能的文件管理基座 |
| �📚 研究工具 | [LeadersPseudoIMBT](#leaderspseudoimbt) | 领导特性分析 |
| 🧪 实验代码 | [AgentLearn](#agentlearn) | 智能体探索 |

---

## 📦 项目详情

### 🤖 LLM 集成与服务

#### **AgentLearn**
🔗 `./AgentLearn/`

智能体、代码编辑器探索的实验代码。包含 MiniClaudeCode 模块，用于探索 AI 辅助编程的各种方法。

---

#### **ErnieToolCallsProxy**
🔗 `./ErnieToolCallsProxy/`

OpenAI 格式的 LLM 代理服务，用于将不支持原生 Tool Calls 功能的模型（如部分开源或定制 LLM）包装为支持 Tool Calls 的接口。

**功能特点：**
- 兼容 OpenAI API 格式
- 自动 Tool Calls 转换
- 完整的测试套件

---

#### **Llama-cpp-ernie-tutorial**
🔗 `./Llama-cpp-ernie-tutorial/`

演示如何将 ERNIE 4.5 模型转换为 GGUF 格式，并使用 llama.cpp 启动兼容 OpenAI API 的服务。

**关键特性：**
- 模型格式转换指南
- OpenAI 兼容 API 服务
- 详细的部署文档

---

#### **OpenCodeServiceswithRknnLllm**
🔗 `./OpenCodeServiceswithRknnLllm/` {#opencodewithrknn}

Qwen2.5-Coder + OpenCode Server on RK3588S。本项目旨在 Rockchip RK3588S (CoolPi 4B) 硬件上部署 Qwen2.5-1.5B-Coder 模型，并将其作为 OpenCode 的后端服务端，提供本地化的 AI 编程助手服务。

**应用场景：**
- 边缘计算设备部署
- 本地化 AI 编程助手
- 低功耗 NPU 推理

---

### 🔍 搜索与排序

#### **OpenAIRerankServer**
🔗 `./OpenAIRerankServer/` {#openaireranksserver}

基于 FastAPI 的重排接口，兼容 OpenAI API 格式，使用本地 CrossEncoder 模型进行文档重排。

**特点：**
- FastAPI 高性能服务
- 本地 CrossEncoder 模型
- OpenAI API 兼容格式

---

### � 文件管理

#### **FileStation**
🔗 `./FileStation/` {#filestation}

一个极简、高性能且面向未来的现代文件管理系统，可以作为文件管理器的基座架构。其轻量化架构非常适合作为个人 NAS 或私有云存储的前端交互界面。

**核心定位：**
- **底层基座**：抽象了物理存储与逻辑路径，支持 Path 和 CAS 两种模式。
- **NAS 扩展性**：提供了标准的文件管理接口，方便集成到各类存储系统。
- **AI Ready**：预留了向量化检索、多模态元数据提取的插件接口。

---

### �🔤 文本识别与视觉理解

#### **PaddleOCR-VL-CPU**
🔗 `./PaddleOCR-VL-CPU/` {#paddleocr-vl-cpu}

基于 PaddleOCR-VL 模型的 CPU 文本图像理解系统。本项目展示了如何在 CPU 环境下部署和使用 PaddleOCR-VL 模型，实现对文本图像的理解和分析功能。

> 💡 **注:** 该项目并非原始贡献，仅在既有项目基础上对代码进行了鲁棒性优化。

---

#### **PaddleOCR-VL-GGUF**
🔗 `./PaddleOCR-VL-GGUF/` {#paddleocr-vl-gguf}

PaddleOCR-VL GGUF (llama.cpp 版)。本项目将多模态模型拆分成「视觉编码器 + 语言模型」两部分，视觉侧保持 PyTorch，语言侧使用 GGUF 量化后通过 llama.cpp 系列工具直接加载，旨在消费级硬件上以最小的内存占用和延迟运行 PaddleOCR-VL。

**优势：**
- 低内存占用
- 快速推理延迟
- 混合格式加载

---

### 📊 性能测试与评估

#### **OpenAIConcurrencyTesting**
🔗 `./OpenAIConcurrencyTesting/` {#openaiconcurrencytesting}

OpenAI 并发测试和 LLM 基准测试工具，用于测试 OpenAI 格式 API 的并发性能和连接稳定性。

**功能模块：**
- 基础连接测试 (`connection_test.py`)
- LLM 基准测试套件 (`llm-benchmark/`)
- 性能数据收集与分析

---

### 📚 研究与分析工具

#### **LeadersPseudoIMBT**
🔗 `./LeadersPseudoIMBT/` {#leaderspseudoimbt}

类 IMBT 领导特性调研与分析工具。本项目介绍了如何基于 Python 和 Gradio 搭建领导特性调研工具。通过一组精心设计的调研问题，结合 ERNIE-4.5-21B-A3B-Thinking 模型的强大分析能力，深入挖掘领导的各种特性倾向（如工作态度、沟通方式、管理风格等）。

**核心功能：**
- Gradio 交互式界面
- 精准问卷设计
- AI 深度分析能力
- 结构化调研流程

---

## 📝 许可证

本项目采用 MIT 许可证。详见 [LICENSE](LICENSE) 文件。