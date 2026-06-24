# OpenAIJsonWrapper

轻量级的 OpenAI 响应解析封装。

## 安装

```bash
npm install openaijsonwrapper
```

## 打包与分发

如果需要从源码构建：

```bash
npm install
npm run build
```

构建完成后，在 `dist/` 目录下会生成编译后的 `.js` 和 `.d.ts` 文件。

## 功能与用法

提供一个 `OpenAIJsonWrapper` 对象，自动注入 System Prompt 并解析模型输出的 JSON。

### 基本用法

```javascript
import OpenAI from "openai";
import { OpenAIJsonWrapper } from "openaijsonwrapper";

const client = new OpenAI({
  apiKey: "sk-...",
  baseURL: "https://api.minimaxi.com/v1"
});

// 定义需要的 JSON 结构
const targetStructure = {
  user_info: {
    name: "string",
    age: "int",
    hobbies: ["string"]
  },
  summary: "string"
};

// 初始化 Wrapper (指定模型和结构)
const wrapper = new OpenAIJsonWrapper(client, {
  model: "MiniMax-M3",
  targetStructure
});

// 正常传入 messages，框架会自动注入 System Prompt
const messages = [
  { role: "user", content: "你好，我是小明，今年 18 岁，喜欢打篮球和听歌。" }
];

const result = await wrapper.chat(messages);

// 获取解析后的结果
if (!result.error) {
  console.log("解析后的数据:", result.data);
  console.log("模型的思维链/正文内容:", result.reasoning);
} else {
  console.log("解析出错:", result.error);
}
```

### 进阶功能

`OpenAIJsonWrapper` 支持在初始化或调用时传入 `background`（背景信息）和 `requirements`（特定需求）。

```javascript
// 在初始化时定义默认配置
const wrapper = new OpenAIJsonWrapper(client, {
  model: "gpt-4o",
  background: "你是一个资深的简历分析专家。",
  requirements: ["提取内容必须客观", "年龄若未知请填 0"]
});

// 在 chat 调用时覆盖或补充配置
const result = await wrapper.chat(messages, {
  targetStructure: newStructure,          // 覆盖默认结构
  extraRequirements: "补充一项新要求",     // 补充到默认需求中
  background: "覆盖初始化时的背景信息"     // 覆盖默认背景
});
```

## 特性

- **自动提示词注入**: 自动在 System Prompt 中包含 JSON 结构定义、背景信息和需求说明。
- **DeepSeek 兼容**: 自动处理 `</think>` 标记，精准分离思维链内容。
- **健壮解析**: 使用正则表达式提取 ` ```json ` 块，具备自动容错（修复末尾逗号等）和回退提取（寻找最后一个合法的 JSON 对象/数组）能力。
- **灵活配置**: 支持在实例级别或调用级别设置结构、需求和背景，支持多需求合并。
- **极简设计**: 专注于 LLM 工具调用/结构化数据提取任务。
- **多模态支持**: 支持 GPT-4V 等 vision 模型，自动处理本地图片路径和 URL，支持结构化图片分析。

### 在 `chat()` 中直接传入图片

`chat()` 兼容 OpenAI 风格的多模态消息 `content`（list 形式），可在单轮/多轮对话中混合文本与图片。

支持的 part 类型：

- `{"type": "text", "text": "..."}` —— 文本片段
- `{"type": "image_url", "image_url": {"url": "http(s)://..." | "data:..."}}` —— 原生 OpenAI 格式
- `{"type": "image_url", "image_url": "path/to/local.jpg"}` —— 字符串形式的本地路径，会被自动 base64 编码
- `{"type": "image_path", "image_path": "path/to/local.jpg"}` —— 便捷写法，传入本地路径（也可以传 URL）

```javascript
const messages = [
  {
    role: "user",
    content: [
      { type: "text", text: "请用 JSON 描述这张图片" },
      { type: "image_path", image_path: "cat.jpg" },
      { type: "image_url", image_url: "https://example.com/dog.png" }
    ]
  }
];

const result = await wrapper.chat(messages);
```

### 多模态图片分析

通过 `chat()` + 多模态 `content` 即可让 vision 模型返回结构化 JSON。

```javascript
import OpenAI from "openai";
import { OpenAIJsonWrapper } from "openaijsonwrapper";

const client = new OpenAI({
  apiKey: "sk-...",
  baseURL: "..."
});

const targetStructure = {
  label: "string (图片分类标签)",
  reason: "string (简短理由)"
};

const wrapper = new OpenAIJsonWrapper(client, {
  model: "gpt-4o",
  targetStructure,
  requirements: ["只能从给定选项中选择"]
});

// 本地图片路径（自动 base64 编码）
const result = await wrapper.chat([
  {
    role: "user",
    content: [
      { type: "text", text: "这张图片属于哪个类别？" },
      { type: "image_path", image_path: "path/to/image.jpg" }
    ]
  }
]);

// 或使用远程 URL
const result2 = await wrapper.chat([
  {
    role: "user",
    content: [
      { type: "text", text: "这张图片属于哪个类别？" },
      { type: "image_url", image_url: { url: "https://example.com/image.jpg" } }
    ]
  }
]);

if (!result.error) {
  console.log(result.data);
} else {
  console.log(result.error);
}
```

## API

### `new OpenAIJsonWrapper(client, options)`

- `client`: OpenAI 风格的客户端（必须具有 `chat.completions.create` 方法）
- `options.model`: 模型名称（默认: "gpt-3.5-turbo"）
- `options.targetStructure`: 默认输出的 JSON 结构
- `options.requirements`: 默认需求（字符串或数组）
- `options.background`: 默认背景上下文

### `wrapper.chat(messages, options)`

- `messages`: 消息对象数组，包含 role 和 content
- `options.targetStructure`: 覆盖默认结构
- `options.requirements`: 覆盖默认需求
- `options.extraRequirements`: 追加到现有需求
- `options.background`: 覆盖默认背景
- `options.model`: 覆盖默认模型

返回 `{ reasoning, data, error, raw_content, response_id }`。

## License

MIT
