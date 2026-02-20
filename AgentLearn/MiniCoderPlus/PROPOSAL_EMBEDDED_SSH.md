# MiniCoder Plus: 嵌入式设备 SSH 适配方案 (Buildroot/Busybox)

此文档旨在为后续开发提供技术蓝图，解决 Agent 在资源受限、缺乏标准文件传输协议（SFTP/SCP）的嵌入式下位机（如 Buildroot 编译的内核环境）中进行代码操作的问题。

## 1. 核心挑战

- **协议缺失**：`Dropbear` 环境常隐掉 `SFTP`，导致标准客户端无法连接。
- **环境碎片化**：`Busybox` 提供的工具集（`ls`, `sed`, `grep`）是精简版，不支持非标准的 GNU 参数。
- **传输脆弱性**：纯 Shell 通讯在处理换行符、特殊符号或二进制数据时极易崩溃。

## 2. 核心技术路径：原声降级逻辑 (Universal SSH Pipe)

我们将开发一个透明的连接器，其逻辑遵循以下优先级：

### A. SFTP/SCP (自动探测层)
- 连接时首先执行 `which sftp-server`。
- 如果可用，优先使用标准二进制流传输，保证最高效率。

### B. Base64 隧道层 (通用层 - 核心)
- 如果 SFTP 缺失，切换至 Base64 模式。
- **读取逻辑**：`ssh target "base64 /path/to/file"` -> 本地接收 -> 本地 Base64 解码。
- **写入逻辑**：本地文件 Base64 编码 -> 分块（Chunking）发送 -> `ssh target "echo 'BASE64_DATA' | base64 -d > /path/to/file"`。
- **优势**：二进制安全，对网络波动和特殊字符（如 Python 的 `'''` 或 Shell 的 `$()`）免疫。

### C. Here-Document 降级层 (极致兼容层)
- 如果目标机连 `base64` 都没有。
- 使用 `cat << 'EOF' > file` 结合手动转义。仅用于极小型的配置文件写入。

## 3. 拟议工具集 (EmbeddedSSHTools)

| 工具名称 | 远程实现原理 (Busybox 兼容版) | 用途 |
| :--- | :--- | :--- |
| `remote_list` | `ls -p1 (按类型并单列)` | 探索目录结构 |
| `remote_read` | `cat file \| base64` | 获取文件内容 |
| `remote_write`| `echo ... \| base64 -d` | 下发布署脚本或项目代码 |
| `remote_exec` | `sh -c "command"` | 运行 Build/Test 命令 |
| `remote_stat` | `df -h / test -f` | 检查可用空间和文件状态 |

## 4. 针对“窗口环境”的考量

未来我们开发的 GUI 窗口将提供：
1. **实时终端映射**：在窗口一侧显示 SSH 原始回显，方便监控 Agent 的行为。
2. **虚拟文件浏览器**：将远程 Buildroot 的目录结构映射到 GUI 侧边栏，虽然底层通过 `ls` 指令同步，但用户体验上像本地一样。
3. **连接管理器**：管理 IP 地址、端口、密钥，支持快速切换不同的下位机。

## 5. 后续开发路线图

1. **Phase 1 (环境探测)**：编写探测脚本，自动生成目标机的“功能画像”（是否有 base64, grep 支持到什么程度）。
2. **Phase 2 (双道驱动)**：实现 `minicoder` 的一套专用接口，能够将 `WorkSpace` 内的代码“热热部署”到嵌入式设备。
3. **Phase 3 (GUI 集成)**：开发完成桌面窗口后，将上述逻辑作为“远程面板”集成进去。

---
**备注**：此方案强调“越原生越稳”，旨在让 MiniCoder 在无需对下位机进行任何修改（No Modification on Target）的情况下实现完全控制。
