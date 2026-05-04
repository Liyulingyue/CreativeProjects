# V-Stage

Voice-Staging Interface — 语音录制 → ASR 识别 → 填充输入框

## 架构

```
┌─────────────────────────────────┐
│      V-Stage (Rust + WebView2)  │  窗口、热键、录音
└──────────────┬──────────────────┘
               │  POST /transcribe
               ▼
┌─────────────────────────────────┐
│   WhisperServer (Python)         │  faster-whisper HTTP API
│   模型常驻内存，识别 1-2 秒       │
└─────────────────────────────────┘
```

WhisperServer 需独立启动，V-Stage 只调用 HTTP 接口。

## 安装

### 1. 安装 Python 依赖

```bash
pip install -r requirements.txt
```

### 2. 构建 Rust 程序

```bash
cargo build --release
```

### 3. 构建前端

```bash
cd ui
npm install
npm run build
cd ..
```

### 4. 部署

将以下文件放到同一目录：

- `v-stage.exe`
- `ui/dist/` 文件夹

```
deploy/
├── v-stage.exe
└── dist/
    └── (前端文件)
```

## 使用

1. 启动 WhisperServer：
   ```bash
   python server/whisper_server.py --model base
   ```
2. 启动 `v-stage.exe`
3. 界面点击录音按钮，或按热键开始录音
4. 再次点击/按热键停止
5. ASR 识别结果展示后，点击"填充"填入输入框

## 配置

点击右上角 ⚙ 进入设置：

| 选项 | 说明 |
|------|------|
| 热键 | F13-F24 / Ctrl+Space |
| ASR 模型 | tiny / base / small / medium |
| 语言 | 自动检测 / 中文 / English 等 |
| 端口 | WhisperServer 监听端口，默认 18789 |

## 模型

Whisper 模型下载到 `%LOCALAPPDATA%\VStage\models\`，可用 `WHISPER_MODEL_DIR` 环境变量修改路径。

## 开发

创建 `.env`：

```bash
cp .env.example .env
```

修改 `.env` 中 `DEV_MODE=1` 启用开发模式。

前端 dev server 也需单独启动：

```bash
# 终端1: Vite dev server
cd ui && npm run dev

# 终端2: Rust app
cargo run
```
