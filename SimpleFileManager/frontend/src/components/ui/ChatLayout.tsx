import { useState, useEffect, useRef } from 'react';
import { Icon } from './Icon';

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
  const diff = Date.now() - timestamp;
  if (diff < 60000) return '刚刚';
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
  if (diff < 604800000) return `${Math.floor(diff / 86400000)} 天前`;
  return d.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function ChatLayout({
  sessions, currentSessionId, onSelectSession, onNewSession,
  onDeleteSession, onSendMessage, isLoading, emptyState, renderMessage,
}: ChatLayoutProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const currentSession = sessions.find(s => s.id === currentSessionId);

  useEffect(() => { messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [currentSession?.messages]);
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
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); handleSubmit(e); }
  };

  const getTitle = (session: ChatSession) => {
    if (session.title) return session.title;
    const firstUserMsg = session.messages.find(m => m.role === 'user');
    if (firstUserMsg) return firstUserMsg.content.slice(0, 20) + (firstUserMsg.content.length > 20 ? '...' : '');
    return '新对话';
  };

  const [sessionsOpen, setSessionsOpen] = useState(false);

  return (
    <div className="flex h-full relative">
      {/* Mobile overlay */}
      {sessionsOpen && (
        <div className="fixed inset-0 z-30 bg-black/30 lg:hidden" onClick={() => setSessionsOpen(false)} />
      )}

      {/* Sessions sidebar — desktop */}
      <div className="hidden lg:flex w-64 bg-white border-r border-slate-200 flex-col flex-shrink-0">
        <SessionList
          sessions={sessions}
          currentSessionId={currentSessionId}
          onNewSession={onNewSession}
          onSelectSession={onSelectSession}
          onDeleteSession={onDeleteSession}
          getTitle={getTitle}
          formatTime={formatTime}
        />
      </div>

      {/* Sessions sidebar — mobile drawer */}
      {sessionsOpen && (
        <div className="fixed left-0 top-0 bottom-0 z-40 w-64 bg-white border-r border-slate-200 flex flex-col lg:hidden shadow-popover">
          <SessionList
            sessions={sessions}
            currentSessionId={currentSessionId}
            onNewSession={onNewSession}
            onSelectSession={(id) => { onSelectSession(id); setSessionsOpen(false); }}
            onDeleteSession={onDeleteSession}
            getTitle={getTitle}
            formatTime={formatTime}
          />
        </div>
      )}

      {/* Chat area */}
      <div className="flex-1 flex flex-col bg-white min-w-0">
        {/* Mobile top bar */}
        <div className="lg:hidden flex items-center gap-2 px-3 py-2 border-b border-slate-100 flex-shrink-0">
          <button
            onClick={() => setSessionsOpen(true)}
            className="p-2 rounded-lg text-slate-500 hover:bg-slate-100"
          >
            <Icon name="chat" size={18} />
          </button>
          <span className="text-sm font-medium text-slate-700 truncate">{currentSession ? getTitle(currentSession) : '新对话'}</span>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {!currentSession || currentSession.messages.length === 0 ? (
            emptyState || <div className="h-full flex items-center justify-center text-slate-400">开始一个新对话吧</div>
          ) : (
            currentSession.messages.map(msg => (
              <div key={msg.id} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
                <div className={`max-w-[70%] rounded-xl px-4 py-2.5 ${
                  msg.role === 'user'
                    ? 'bg-indigo-600 text-white'
                    : 'bg-slate-100 text-slate-800'
                }`}>
                  {renderMessage ? renderMessage(msg) : (
                    <div className="whitespace-pre-wrap text-sm leading-relaxed">{msg.content}</div>
                  )}
                  {msg.sources && msg.sources.length > 0 && (
                    <div className={`mt-2 pt-2 border-t text-xs space-y-0.5 ${msg.role === 'user' ? 'border-indigo-500' : 'border-slate-200'}`}>
                      <div className="opacity-70 mb-1">参考文档：</div>
                      {msg.sources.map((s, i) => (
                        <div key={i} className="opacity-60 flex items-center gap-1.5">
                          <Icon name="file" size={12} />
                          {s.file_path.split('/').pop()} ({(s.score * 100).toFixed(0)}%)
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
              <div className="bg-slate-100 rounded-xl px-4 py-2.5">
                <div className="flex items-center gap-2 text-slate-500 text-sm">
                  <div className="flex gap-1">
                    <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
                    <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
                    <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
                  </div>
                  <span>思考中...</span>
                </div>
              </div>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        <div className="border-t border-slate-100 p-3 flex-shrink-0">
          <form onSubmit={handleSubmit} className="flex items-end gap-2">
            <textarea
              ref={inputRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入问题... (Shift+Enter 换行)"
              rows={1}
              className="flex-1 px-3 py-2 rounded-lg border border-slate-200 text-sm resize-none focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
              style={{ maxHeight: '150px' }}
            />
            <button
              type="submit"
              disabled={!input.trim() || isLoading}
              className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              <Icon name="send" size={16} />
              发送
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}

function SessionList({
  sessions, currentSessionId, onNewSession, onSelectSession, onDeleteSession, getTitle, formatTime,
}: {
  sessions: ChatSession[];
  currentSessionId: string | null;
  onNewSession: () => Promise<string>;
  onSelectSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
  getTitle: (s: ChatSession) => string;
  formatTime: (t: number) => string;
}) {
  return (
    <>
      <div className="p-3 border-b border-slate-100">
        <button
          onClick={() => onNewSession()}
          className="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
        >
          <Icon name="plus" size={16} />
          新建对话
        </button>
      </div>
      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        {sessions.length === 0 ? (
          <div className="p-4 text-center text-sm text-slate-400">暂无对话记录</div>
        ) : sessions.map(session => (
          <div
            key={session.id}
            className={`group relative rounded-lg transition-colors ${
              session.id === currentSessionId ? 'bg-indigo-50' : 'hover:bg-slate-100'
            }`}
          >
            <button onClick={() => onSelectSession(session.id)} className="w-full text-left px-3 py-2.5 pr-8">
              <div className="text-sm font-medium text-slate-700 truncate">{getTitle(session)}</div>
              <div className="text-xs text-slate-400 mt-0.5">{formatTime(session.updated_at)}</div>
            </button>
            <button
              onClick={(e) => { e.stopPropagation(); onDeleteSession(session.id); }}
              className="absolute right-2 top-1/2 -translate-y-1/2 w-6 h-6 rounded text-slate-400 hover:text-red-500 hover:bg-red-50 opacity-0 group-hover:opacity-100 transition-all flex items-center justify-center"
              title="删除对话"
            >
              <Icon name="trash" size={14} />
            </button>
          </div>
        ))}
      </div>
    </>
  );
}
