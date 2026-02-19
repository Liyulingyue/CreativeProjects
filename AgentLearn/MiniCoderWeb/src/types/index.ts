// TypeScript类型定义

export interface CodeResult {
  content: string;
  language?: string;
  explanation?: string;
}

export interface CodeRequest {
  prompt: string;
  language?: string;
  code?: string;
  errorContext?: string;
}

export interface ApiResponse {
  success: boolean;
  data?: CodeResult;
  error?: string;
  message?: string;
}