import { useState, useEffect, useRef } from 'react';
import { sendAgentMessage, type AgentMessage } from '../api';

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export function SimpleChat() {
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [input, setInput] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [sessionId] = useState(() => generateId());
  const [tools, setTools] = useState<string[]>([]);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
      inputRef.current.style.height = Math.min(inputRef.current.scrollHeight, 150) + 'px';
    }
  }, [input]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    const userMessage: AgentMessage = {
      id: generateId(),
      role: 'user',
      content: input.trim(),
      timestamp: Date.now(),
    };

    setMessages(prev => [...prev, userMessage]);
    setInput('');
    setIsLoading(true);

    try {
      const response = await sendAgentMessage(userMessage.content, sessionId);

      const assistantMessage: AgentMessage = {
        id: generateId(),
        role: 'assistant',
        content: response.response,
        tool_calls: response.tool_results?.map(tr => ({
          name: tr.tool,
          arguments: tr.arguments,
          result: tr.result,
        })),
        timestamp: Date.now(),
      };

      setMessages(prev => [...prev, assistantMessage]);

      if (response.available_tools?.length && !tools.length) {
        setTools(response.available_tools);
      }
    } catch (error) {
      const errorMessage: AgentMessage = {
        id: generateId(),
        role: 'assistant',
        content: '发生错误：' + (error instanceof Error ? error.message : '未知错误'),
        timestamp: Date.now(),
      };
      setMessages(prev => [...prev, errorMessage]);
    } finally {
      setIsLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      {/* Header */}
      <div className="bg-white border-b border-slate-200 px-6 py-3">
        <div className="flex items-center gap-4">
          <div className="text-xl font-bold text-indigo-600">🤖 Agent</div>
          {tools.length > 0 && (
            <div className="text-xs text-slate-400">
              可用工具: {tools.join(', ')}
            </div>
          )}
        </div>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {messages.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-slate-400 space-y-4">
            <div className="text-5xl">🤖</div>
            <div className="text-center">
              <div className="font-medium text-slate-600 mb-1">Agent 对话</div>
              <div className="text-sm">我可以执行命令、读写文件、搜索内容</div>
            </div>
          </div>
        )}

        {messages.map(msg => (
          <div
            key={msg.id}
            className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
          >
            <div
              className={`max-w-[80%] rounded-2xl px-4 py-3 ${
                msg.role === 'user'
                  ? 'bg-indigo-600 text-white rounded-br-md'
                  : msg.role === 'tool'
                  ? 'bg-orange-100 text-orange-800 rounded-bl-md'
                  : 'bg-white text-slate-800 rounded-bl-md border border-slate-200'
              }`}
            >
              {msg.role === 'tool' && (
                <div className="text-xs font-bold mb-1 text-orange-600">
                  🔧 工具: {msg.content}
                </div>
              )}
              {msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0 && (
                <div className="mb-2 pb-2 border-b border-slate-200">
                  <div className="text-xs text-slate-500 mb-1">调用的工具：</div>
                  {msg.tool_calls.map((tc, i) => (
                    <div key={i} className="text-xs bg-slate-50 rounded p-1 mb-1">
                      <span className="font-bold text-indigo-600">{tc.name}</span>
                      <span className="text-slate-500 ml-2">
                        {JSON.stringify(tc.arguments).slice(0, 50)}...
                      </span>
                    </div>
                  ))}
                </div>
              )}
              <div className="text-sm leading-relaxed whitespace-pre-wrap">
                {msg.content}
              </div>
            </div>
          </div>
        ))}

        {isLoading && (
          <div className="flex justify-start">
            <div className="bg-white rounded-2xl rounded-bl-md px-4 py-3 border border-slate-200">
              <div className="flex items-center gap-2 text-slate-500 text-sm">
                <div className="flex gap-1">
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                  <span className="w-2 h-2 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                </div>
                <span>思考中...</span>
              </div>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div className="border-t border-slate-200 p-4 bg-white">
        <form onSubmit={handleSubmit} className="flex items-end gap-3">
          <div className="flex-1">
            <textarea
              ref={inputRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入指令... (Enter 发送, Shift+Enter 换行)"
              rows={1}
              className="w-full px-4 py-3 rounded-xl border border-slate-200 bg-slate-50 text-sm resize-none focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
              style={{ maxHeight: '150px' }}
            />
          </div>
          <button
            type="submit"
            disabled={!input.trim() || isLoading}
            className="px-6 py-3 rounded-xl bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            发送
          </button>
        </form>
      </div>
    </div>
  );
}
