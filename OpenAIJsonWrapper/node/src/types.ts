export interface ContentPart {
  type: "text" | "image_url" | "image_path";
  text?: string;
  image_url?: string | { url: string; detail?: "low" | "high" | "auto" };
  image_path?: string;
}

export interface Message {
  role: "system" | "user" | "assistant" | "developer";
  content: string | ContentPart[];
}

export interface ChatResult {
  reasoning: string;
  data: Record<string, any> | null;
  error: string | null;
  raw_content: string;
  response_id?: string;
}

export interface OpenAIJsonWrapperOptions {
  client: any;
  model?: string;
  targetStructure?: any;
  requirements?: string | string[];
  background?: string;
}

const TOOL_MARKER_START = "```json";
const TOOL_MARKER_END = "```";
