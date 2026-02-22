export interface Message {
  role: 'user' | 'assistant' | 'tool';
  content: string;
  isThought?: boolean;
  tool_calls?: any[];
  tool_call_id?: string;
  name?: string;
}
