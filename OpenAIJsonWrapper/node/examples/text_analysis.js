import OpenAI from "openai";
import { OpenAIJsonWrapper } from "../src/index.js";

const apiKey = process.env.OPENAI_API_KEY || "your-api-key-here";
const baseUrl = process.env.OPENAI_BASE_URL || "https://api.minimaxi.com/v1";
const modelName = process.env.OPENAI_MODEL_NAME || "MiniMax-M3";

if (apiKey === "your-api-key-here") {
  console.log("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。");
}

const client = new OpenAI({
  apiKey,
  baseURL: baseUrl
});

const targetStructure = {
  analysis: {
    sentiment: "string (Positive/Negative/Neutral)",
    key_entities: ["string"],
    confidence_score: "float (0-1)"
  },
  response_suggestion: "string"
};

const wrapper = new OpenAIJsonWrapper(client, {
  model: modelName,
  targetStructure
});

const messages = [
  { role: "user", content: "GitHub Copilot 是一款非常棒的 AI 编程助手，它能极大地提高代码生产力，虽然偶尔会有小瑕疵，但整体瑕不掩瑜。" }
];

const requirements = [
  "情感分析必须细化到分值",
  "key_entities 至少提取两个",
  "response_suggestion 必须是中文"
];

const background = "你是一个专业级的产品评论分析专家，擅长从用户反馈中提取结构化洞察。";

async function main() {
  console.log("--- 正在发送请求到 LLM ---");

  const result = await wrapper.chat(messages, {
    requirements,
    background
  });

  console.log("\n--- 解析结果 ---");
  if (!result.error) {
    console.log("成功解析数据:");
    console.log(JSON.stringify(result.data, null, 2));
    console.log("\n思维链/推理过程:");
    console.log(result.reasoning);
  } else {
    console.log("解析失败!");
    console.log("错误信息:", result.error);
    console.log("原始响应内容:\n", result.raw_content);
  }
}

main();
