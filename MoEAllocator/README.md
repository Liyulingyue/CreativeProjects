# MoEAllocator (MoE 算力调度器)

**MoEAllocator** 是一个面向异构硬件环境（如台式机、NUC、树莓派等）的自动化大模型分布式推理框架。它专为 **混合专家模型 (Mixture of Experts)** 设计，通过“外科手术”级的权重拆解与动态代理技术，将庞大的模型能力分摊到多台边缘设备上。



## 核心愿景
在拥有 32GB+ 内存的台式机与 4GB/8GB 内存的树莓派之间，打破物理内存的壁垒。**MoEAllocator** 让你可以将主干网络保留在性能最强的机器上，而将数十个专家（Experts）像“计算微服务”一样部署在网络中的任何角落。

---

## 核心特性

* **🧪 自动化模型手术 (Automated Sharding)**：无需手动修改模型代码。系统自动扫描 `transformers` 模型架构，识别专家层并执行物理层面的权重切片。
* **📡 动态层代理 (Dynamic Layer Proxying)**：利用 Python 的动态特性，在运行时将本地专家模块替换为 TCP 远程代理。对于模型主干而言，远程专家是“透明”的。
* **🔗 异构拓扑感知 (Heterogeneous Topology Awareness)**：支持根据节点内存容量自动分配专家数量。例如，给 Ultra 5 NUC 分配 4 个专家，给树莓派分配 1 个专家。
* **⚡ 零内存冗余策略 (Zero-Footprint Strategy)**：主机（Master）可以被强制配置为“最低限度运行”模式，仅保留 Embedding、Attention 和极少数关键专家，从而在 16GB 甚至更小的设备上跑起 21B+ 规模的模型。
* **🛠️ 架构普适性**：原生支持 Mixtral、DeepSeek、MiniCPM、Ernie-21B (A3B) 等主流 MoE 架构。

---

## 技术架构

`MoEAllocator` 的工作流程分为三个阶段：

1.  **扫描 (Inquisitor)**：分析 `safetensors` 索引，通过正则表达式自动化定位 `gate` 与 `experts` 的权重路径。
2.  **分发 (Surgeon)**：将切片后的专家权重通过 `TCP/HTTP` 自动分发至已注册的计算节点（Workers）。
3.  **接管 (Nexus)**：主控端通过拦截 `forward` 调用，将特定 Token 的 Hidden States 通过高效的二进制流（Raw Buffer over TCP）发送至对应节点并取回结果。



---

## 快速开始 (开发中)

### 1. 节点配置
在 `config/nodes.yaml` 中定义你的计算阵列：
```yaml
nodes:
  - id: "master-i7"
    address: "localhost"
    keep_experts: 1
  - id: "nuc-ultra5"
    address: "192.168.1.5"
    capacity: "8GB"
  - id: "pi-5"
    address: "192.168.1.10"
    capacity: "4GB"
```

### 2. 自动化拆解与分发
```bash
python main.py --model "openbmb/MiniCPM-MoE-8x2b" --action "allocate"
```

---

## 为什么选择 MoEAllocator？

传统的分布式推理（如张量并行）由于每一层都需要进行高频的内存对齐，在家庭局域网（千兆网）环境下几乎不可用。**MoEAllocator** 利用 MoE 的稀疏性，仅在路由命中远程专家时才进行最少量（Hidden States 向量）的通信，这让**“用树莓派当显存用”**成为了工程上的现实。

---

## 路线图 (Roadmap)

- [ ] 支持基于 `safetensors` 的零内存权重切片。
- [ ] 实现针对不同硬件（NPU/CPU/GPU）的算子自适应分发。
- [ ] 引入“投机专家延迟容忍”机制，防止慢节点拖累全局。
- [ ] 支持 DeepSeek-V3 的大规模专家池调度。

---

**MoEAllocator**：让你的闲置算力，重塑大模型的边界。