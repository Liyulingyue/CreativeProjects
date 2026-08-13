import { useState, useEffect, useRef } from 'react';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  sources?: { file_path: string; score: number }[];
  timestamp: number;
}

export interface ChatSession {
  id: string;
  title: string;
  messages: ChatMessage[];
  updated_at: number;
  session_type?: string;
}

interface ChatLayoutProps {
  sessions: ChatSession[];
  currentSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => Promise<string>;
  onDeleteSession: (id: string) => void;
  onSendMessage: (content: string) => Promise<void>;
  isLoading: boolean;
  emptyState?: React.ReactNode;
  renderMessage?: (message: ChatMessage) => React.ReactNode;
}

function formatTime(timestamp: number): string {
  const d = new Date(timestamp);
  const now = new Date();
  const diff = now.getTime() - timestamp;

  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
  if (diff < 604800000) return `${Math.floor(diff / 86400000)} 天前`;

  return d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function ChatLayout({
  sessions,
  currentSessionId,
  onSelectSession,
  onNewSession,
  onDeleteSession,
  onSendMessage,
  isLoading,
  emptyState,
  renderMessage,
}: ChatLayoutProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const currentSession = sessions.find(s => s.id === currentSessionId);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [currentSession?.messages]);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
      inputRef.current.style.height = Math.min(inputRef.current.scrollHeight, 150) + 'px';
    }
  }, [input]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;

    const content = input.trim();
    setInput('');
    await onSendMessage(content);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  const getTitle = (session: ChatSession) => {
    if (session.title) return session.title;
    const firstUserMsg = session.messages.find(m => m.role === 'user');
    if (firstUserMsg) {
      return firstUserMsg.content.slice(0, 20) + (firstUserMsg.content.length > 20 ? '...' : '');
    }
    return '新对话';
  };

  return (
    <div className="flex h-full">
      {/* Sidebar */}
      <div className="w-64 bg-slate-50 border-r border-slate-200 flex flex-col">
        <div className="p-3 border-b border-slate-200">
          <button
            onClick={() => onNewSession()}
            className="w-full px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors flex items-center justify-center gap-2"
          >
            <span className="text-base">+</span>
            <span>新建对话</span>
          </button>
        </div>

        <div className="flex-1 overflow-y-auto">
          {sessions.length === 0 ? (
            <div className="p-4 text-center text-sm text-slate-400">
              暂无对话记录
            </div>
          ) : (
            <div className="p-2 space-y-1">
              {sessions.map(session => (
                <div
                  key={session.id}
                  className={`group relative rounded-lg transition-colors ${
                    session.id === currentSessionId
                      ? 'bg-indigo-50 border border-indigo-200'
                      : 'hover:bg-slate-100 border border-transparent'
                  }`}
                >
                  <button
                    onClick={() => onSelectSession(session.id)}
                    className="w-full text-left px-3 py-2.5 pr-8"
                  >
                    <div className="text-sm font-medium text-slate-700 truncate">
                      {getTitle(session)}
                    </div>
                    <div className="text-xs text-slate-400 mt-0.5">
                      {formatTime(session.updated_at)}
                    </div>
                  </button>

                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onDeleteSession(session.id);
                    }}
                    className="absolute right-2 top-1/2 -translate-y-1/2 w-6 h-6 rounded text-slate-400 hover:text-red-500 hover:bg-red-50 opacity-0 group-hover:opacity-100 transition-all flex items-center justify-center text-xs"
                    title="删除对话"
                  >
                    ✕
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Main Chat Area */}
      <div className="flex-1 flex flex-col bg-white">
        <>
          {/* Messages */}
          <div className="flex-1 overflow-y-auto p-4 space-y-4">
            {!currentSession || currentSession.messages.length === 0 ? (
              emptyState || (
                <div className="h-full flex items-center justify-center text-slate-400">
                  开始一个新对话吧
                </div>
              )
            ) : (
              currentSession.messages.map(msg => (
                <div
                  key={msg.id}
                  className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}
                >
                  <div
                    className={`max-w-[70%] rounded-2xl px-4 py-3 ${
                      msg.role === 'user'
                        ? 'bg-indigo-600 text-white rounded-br-md'
                        : 'bg-slate-100 text-slate-800 rounded-bl-md'
                    }`}
                  >
                    {renderMessage ? (
                      renderMessage(msg)
                    ) : (
                      <div className="whitespace-pre-wrap text-sm leading-relaxed">
                        {msg.content}
                      </div>
                    )}

                    {msg.sources && msg.sources.length > 0 && (
                      <div className={`mt-2 pt-2 border-t ${
                        msg.role === 'user' ? 'border-indigo-500' : 'border-slate-200'
                      }`}>
                        <div className="text-xs opacity-70 mb-1">参考文档：</div>
                        {msg.sources.map((s, i) => (
                          <div key={i} className="text-xs opacity-60 py-0.5">
                            📄 {s.file_path.split('/').pop()} ({(s.score * 100).toFixed(0)}%)
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              ))
            )}

            {isLoading && (
              <div className="flex justify-start">
                <div className="bg-slate-100 rounded-2xl rounded-bl-md px-4 py-3">
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

          {/* Input Area */}
          <div className="border-t border-slate-200 p-4">
            <form onSubmit={handleSubmit} className="flex items-end gap-3">
              <div className="flex-1 relative">
                <textarea
                  ref={inputRef}
                  value={input}
                  onChange={e => setInput(e.target.value)}
                  onKeyDown={handleKeyDown}
                  placeholder="输入问题... (Shift+Enter 换行)"
                  rows={1}
                  className="w-full px-4 py-3 rounded-xl border border-slate-200 bg-slate-50 text-sm resize-none focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                  style={{ maxHeight: '150px' }}
                />
              </div>
              <button
                type="submit"
                disabled={!input.trim() || isLoading}
                className="px-5 py-3 rounded-xl bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center gap-2"
              >
                <span>发送</span>
                <span className="text-base">↑</span>
              </button>
            </form>
          </div>
        </>
      </div>
    </div>
  );
}
