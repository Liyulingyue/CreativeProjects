export interface Message {
  role: 'user' | 'assistant' | 'tool';
  content: string;
  isThought?: boolean;
  tool_calls?: any[];
  tool_call_id?: string;
  name?: string;
  // Database message ID for feedback/evaluation
  id?: number;
  // User feedback on this message ('👍', '👎', etc.)
  feedback?: string;
  // User's comment on the feedback
  feedback_comment?: string;
}

export interface FileItem {
  name: string;
  path: string;
  abs_path: string;
  type: 'file' | 'directory';
}
