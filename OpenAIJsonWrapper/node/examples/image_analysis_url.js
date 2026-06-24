import OpenAI from "openai";
import { OpenAIJsonWrapper } from "../src/index.js";

const apiKey = process.env.OPENAI_API_KEY || "your-api-key-here";
const baseUrl = process.env.OPENAI_BASE_URL || "https://api.minimaxi.com/v1";
const modelName = process.env.OPENAI_VISION_MODEL_NAME || "MiniMax-M3";

const imagePath = process.env.TEST_IMAGE_PATH || "path/to/image.jpg";
const imageUrl = process.env.TEST_IMAGE_URL || "https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/Cat03.jpg/1200px-Cat03.jpg";

async function testImageUrl() {
  if (apiKey === "your-api-key-here") {
    console.log("警告: 未检测到 OPENAI_API_KEY 环境变量，请确保已设置或手动修改脚本逻辑。");
  }

  const client = new OpenAI({
    apiKey,
    baseURL: baseUrl
  });

  const targetStructure = {
    score: "int, 0-100, 代表照片质量评分",
    style: "str, 照片风格描述",
    caption: "str, 用中文写一句话，不超过 30 字",
    main_objects: "list[str], 至少 2 个主要物体",
    blurry: "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰' 三选一",
    comments: "str, 对照片的详细评价，至少 50 字",
    recommendations: "str, 对拍摄者的改进建议，至少 30 字",
  };

  const wrapper = new OpenAIJsonWrapper(client, {
    model: modelName,
    targetStructure,
    background: "你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。",
    requirements: [
      "照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
      "请确保输出的 JSON 严格符合指定的结构和类型要求。",
    ]
  });

  const messages = [
    {
      role: "user",
      content: [
        { type: "text", text: "请仔细观察这张图片，按指定 JSON 结构输出。" },
        { type: "image_url", image_url: imageUrl }
      ]
    }
  ];

  console.log("--- [image_url] 正在发送多模态请求 ---");

  const result = await wrapper.chat(messages);

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

testImageUrl();
