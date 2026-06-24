import { OpenAIJsonWrapper } from "openaijsonwrapper";

export interface AnalysisResult {
  file: string;
  data: PhotoAnalysis | null;
  reasoning: string;
  error: string | null;
  success: boolean;
}

export interface PhotoAnalysis {
  score: number;
  style: string;
  caption: string;
  main_objects: string[];
  blurry: string;
  comments: string;
  recommendations: string;
}

export interface AnalyzerConfig {
  apiKey: string;
  baseUrl: string;
  model: string;
  delay: number;
  maxCacheCount: number;
}

const TARGET_STRUCTURE = {
  score: "int, 0-100, 代表照片质量评分",
  style: "str, 照片风格描述",
  caption: "str, 用中文写一句话，不超过 30 字",
  main_objects: "list[str], 至少 2 个主要物体",
  blurry: "str, 照片是否模糊，'模糊'、'略微模糊'、'清晰' 三选一",
  comments: "str, 对照片的详细评价，至少 50 字",
  recommendations: "str, 对拍摄者的改进建议，至少 30 字",
};

const BACKGROUND = "你是一名专业的旅行照片分析师，擅长从图片中分析出丰富的细节和信息。";

const REQUIREMENTS = [
  "照片的评价评分需要基于照片的清晰度、构图、色彩和主题等因素综合评定。",
  "请确保输出的 JSON 严格符合指定的结构和类型要求。",
];

class SimpleClient {
  constructor(
    private apiKey: string,
    private baseUrl: string
  ) {}

  chat = {
    completions: {
      create: async (params: { model: string; messages: any[] }) => {
        const url = `${this.baseUrl}/chat/completions`;
        const response = await fetch(url, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            Authorization: `Bearer ${this.apiKey}`,
          },
          body: JSON.stringify(params),
        });

        if (!response.ok) {
          throw new Error(`API 请求失败: ${response.status}`);
        }

        return response.json();
      }
    }
  };
}

export async function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

export async function analyzeImage(
  file: File,
  config: AnalyzerConfig
): Promise<AnalysisResult> {
  const { apiKey, baseUrl, model } = config;

  try {
    const client = new SimpleClient(apiKey, baseUrl);

    const wrapper = new OpenAIJsonWrapper(client as any, {
      model,
      targetStructure: TARGET_STRUCTURE,
      background: BACKGROUND,
      requirements: REQUIREMENTS,
    });

    const imageData = await fileToBase64(file);

    const result = await wrapper.chat(
      [
        {
          role: "user",
          content: [
            { type: "text", text: "请仔细观察这张图片，按指定 JSON 结构输出。" },
            { type: "image_url", image_url: { url: imageData } },
          ],
        },
      ],
      { model }
    );

    return {
      file: file.name,
      data: result.data as PhotoAnalysis | null,
      reasoning: result.reasoning || "",
      error: result.error,
      success: !result.error && !!result.data,
    };
  } catch (e) {
    return {
      file: file.name,
      data: null,
      reasoning: "",
      error: e instanceof Error ? e.message : "未知错误",
      success: false,
    };
  }
}

export async function analyzeImages(
  files: File[],
  config: AnalyzerConfig,
  onProgress?: (current: number, total: number) => void
): Promise<AnalysisResult[]> {
  const results: AnalysisResult[] = [];

  for (let i = 0; i < files.length; i++) {
    const result = await analyzeImage(files[i], config);
    results.push(result);
    onProgress?.(i + 1, files.length);

    if (i < files.length - 1 && config.delay > 0) {
      await new Promise((r) => setTimeout(r, config.delay));
    }
  }

  return results;
}

export function exportToJson(results: AnalysisResult[]): void {
  const blob = new Blob([JSON.stringify(results, null, 2)], { type: "application/json" });
  downloadBlob(blob, "analysis_results.json");
}

export function exportToCsv(results: AnalysisResult[]): void {
  const headers = ["file_name", "success", "score", "style", "caption", "blurry", "comments"];
  const rows = results.map((r) => [
    r.file,
    r.success,
    r.data?.score ?? "",
    r.data?.style ?? "",
    r.data?.caption ?? "",
    r.data?.blurry ?? "",
    (r.data?.comments ?? "").replace(/"/g, '""'),
  ]);
  const csv = [headers.join(","), ...rows.map((r) => r.map((v) => `"${v}"`).join(","))].join("\n");
  const blob = new Blob(["\ufeff" + csv], { type: "text/csv;charset=utf-8" });
  downloadBlob(blob, "analysis_results.csv");
}

function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}
