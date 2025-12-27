# CreativeProjects
这是Liyulingyue的创意项目合集。

## 当前项目
- LeadersPseudoIMBT: 类IMBT领导特性调研与分析工具，本项目介绍了如何基于 Python 和 Gradio 搭建领导特性调研工具。通过一组精心设计的调研问题，结合 ERNIE-4.5-21B-A3B-Thinking 模型的强大分析能力，深入挖掘领导的各种特性倾向（如工作态度、沟通方式、管理风格等）。
- PaddleOCR-VL-CPU: 基于PaddleOCR-VL模型的CPU文本图像理解系统，本项目展示了如何在CPU环境下部署和使用PaddleOCR-VL模型，实现对文本图像的理解和分析功能。（该项目并非原始贡献，仅在既有项目基础上对代码进行了鲁棒性优化）
- PaddleOCR-VL-GGUF: PaddleOCR-VL GGUF (llama.cpp 版)，本项目将多模态模型拆分成「视觉编码器 + 语言模型」两部分，视觉侧保持 PyTorch，语言侧使用 GGUF 量化后通过 llama.cpp 系列工具直接加载，旨在消费级硬件上以最小的内存占用和延迟运行 PaddleOCR-VL。