import type { Message, ContentPart, ChatResult } from "./types.js";

const TOOL_MARKER_START = "```json";
const TOOL_MARKER_END = "```";

export class OpenAIJsonWrapper {
  private client: any;
  private model: string;
  private targetStructure: any;
  private requirements: string[];
  private background: string | undefined;

  constructor(client: any, options: {
    model?: string;
    targetStructure?: any;
    requirements?: string | string[];
    background?: string;
  } = {}) {
    this.client = client;
    this.model = options.model || "gpt-3.5-turbo";
    this.targetStructure = options.targetStructure || null;
    this.requirements = this.toList(options.requirements);
    this.background = options.background;
  }

  private toList(x: string | string[] | undefined): string[] {
    if (!x) return [];
    return Array.isArray(x) ? x : [x];
  }

  private buildSystemPrompt(targetStructure: any, requirements?: string[], background?: string): string {
    const structureStr = JSON.stringify(targetStructure, null, 2);
    let prompt = "You are a helpful assistant that MUST output your response in a specific JSON format.\n";

    if (background) {
      prompt += `\nBackground Information:\n${background}\n`;
    }

    prompt += `\nThe required JSON structure is:\n${structureStr}\n\n`;

    if (requirements && requirements.length > 0) {
      const reqText = requirements.map(r => `- ${r}`).join("\n");
      prompt += `Specific Requirements:\n${reqText}\n\n`;
    }

    prompt += (
      "Rules:\n" +
      `1. Your final JSON data MUST be wrapped between '${TOOL_MARKER_START}' and '${TOOL_MARKER_END}' markdown blocks.\n` +
      "2. Everything before the code block is considered your reasoning or conversational text.\n" +
      "3. Ensure the JSON inside the block is valid and matches the requested structure strictly.\n"
    );

    return prompt;
  }

  private parseContent(text: string): { reasoning: string; data: any; error: string | null } {
    if (!text) {
      return { reasoning: "", data: null, error: "Empty content" };
    }

    // Handle thinking: if </think> exists, only keep content after it
    const thinkEndMarker = "</think>";
    if (text.includes(thinkEndMarker)) {
      const parts = text.split(thinkEndMarker);
      text = parts[parts.length - 1].trim();
    }

    // 1) Look for markdown JSON block
    const mdPattern = /```json\s*([\s\S]*?)\s*```/;
    const match = text.match(mdPattern);
    if (match) {
      const inner = match[1].trim();
      const reasoning = text.slice(0, match.index).trim();
      try {
        const parsed = JSON.parse(inner);
        return { reasoning, data: parsed, error: null };
      } catch (e: any) {
        try {
          const cleaned = inner.replace(/,(\s*[}\]])/g, "$1");
          const parsed = JSON.parse(cleaned);
          return { reasoning, data: parsed, error: null };
        } catch {
          return { reasoning, data: null, error: `JSON parse error in markdown block: ${e.message}` };
        }
      }
    }

    // 2) Fallback: try to extract last JSON structure from text
    for (const [opener, closer] of [["\\[", "\\]"], ["\\{", "\\}"]] as const) {
      const idx = text.lastIndexOf(opener);
      if (idx !== -1) {
        let cand = text.slice(idx).trim();
        const lastCloserIdx = cand.lastIndexOf(closer);
        if (lastCloserIdx !== -1) {
          cand = cand.slice(0, lastCloserIdx + 1);
        }
        try {
          const parsed = JSON.parse(cand);
          const reasoning = text.slice(0, idx).trim();
          return { reasoning, data: parsed, error: null };
        } catch {
          continue;
        }
      }
    }

    return { reasoning: text, data: null, error: "No JSON structure found" };
  }

  private async encodeImage(imageSource: string): Promise<string> {
    if (imageSource.startsWith("http://") || imageSource.startsWith("https://")) {
      return imageSource;
    }

    try {
      const response = await fetch(`file://${imageSource}`);
      const buffer = await response.arrayBuffer();
      const base64 = btoa(String.fromCharCode(...new Uint8Array(buffer)));
      const ext = imageSource.split(".").pop()?.toLowerCase() || "jpeg";
      const mimeMap: Record<string, string> = {
        jpg: "image/jpeg",
        jpeg: "image/jpeg",
        png: "image/png",
        gif: "image/gif",
        webp: "image/webp",
        bmp: "image/bmp",
      };
      const mime = mimeMap[ext] || "image/jpeg";
      return `data:${mime};base64,${base64}`;
    } catch {
      throw new Error(`Failed to encode image: ${imageSource}`);
    }
  }

  private async normalizeContentPart(part: ContentPart): Promise<ContentPart> {
    if (part.type === "image_path" && part.image_path) {
      const url = await this.encodeImage(part.image_path);
      return { type: "image_url", image_url: { url } };
    }
    return part;
  }

  private async normalizeMessage(message: Message): Promise<Message> {
    if (Array.isArray(message.content)) {
      const newContent = await Promise.all(message.content.map(p => this.normalizeContentPart(p as ContentPart)));
      return { ...message, content: newContent };
    }
    return message;
  }

  async chat(
    messages: Message[],
    options: {
      targetStructure?: any;
      requirements?: string | string[];
      extraRequirements?: string | string[];
      background?: string;
      model?: string;
    } = {}
  ): Promise<ChatResult> {
    const target = options.targetStructure ?? this.targetStructure;
    if (!target) {
      throw new Error("targetStructure must be provided");
    }

    const reqs = this.toList(options.requirements || []);
    const extraReqs = this.toList(options.extraRequirements || []);
    const combinedReqs = [...this.requirements, ...reqs, ...extraReqs].filter(Boolean);
    const bg = options.background ?? this.background;
    const model = options.model ?? this.model;

    const systemPrompt = this.buildSystemPrompt(target, combinedReqs.length > 0 ? combinedReqs : undefined, bg);

    const newMessages: Message[] = [];
    let hasSystem = false;

    for (const m of messages) {
      if (m.role === "system") {
        newMessages.push({
          role: "system",
          content: `${systemPrompt}\n\n${typeof m.content === "string" ? m.content : JSON.stringify(m.content)}`
        });
        hasSystem = true;
      } else {
        newMessages.push(await this.normalizeMessage(m));
      }
    }

    if (!hasSystem) {
      newMessages.unshift({ role: "system", content: systemPrompt });
    }

    const response = await this.client.chat.completions.create({
      model,
      messages: newMessages as any,
    });

    const choice = response.choices[0];
    const content = choice.message.content || "";

    const { reasoning, data, error } = this.parseContent(content);

    return {
      reasoning,
      data,
      error,
      raw_content: content,
      response_id: response.id,
    };
  }
}

export default OpenAIJsonWrapper;
