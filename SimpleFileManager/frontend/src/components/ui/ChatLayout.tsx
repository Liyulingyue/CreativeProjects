import { useState, useEffect, useRef } from 'react';
import { Icon } from './Icon';
import { useCollapse } from '../../hooks/useCollapse';
import { MarkdownText } from './MarkdownText';
// Reuse the api layer's types. They were duplicated here, which let the two
// definitions drift apart silently until the session prop stopped type-checking.
import type { ChatMessage, ChatSession } from '../../api';

export type { ChatMessage, ChatSession };

/**
 * One turn's transcript as it actually happened: reasoning blocks and tool
 * calls interleaved in arrival order. Kept as a single ordered list because
 * splitting them into "thinking" and "tools" loses the interleaving, which is
 * the part that explains *why* each tool call happened.
 */
export interface ToolActivity {
  kind: 'reasoning' | 'tool';
  tool?: string;
  status?: 'running' | 'done';
  args?: Record<string, unknown>;
  result?: unknown;
  text?: string;
}

const MAX_RESULT_CHARS = 800;

function stringify(value: unknown): string {
  try {
    const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    if (text === undefined) return '';
    return text.length > MAX_RESULT_CHARS ? text.slice(0, MAX_RESULT_CHARS) + '\n… 已截断' : text;
  } catch {
    return String(value);
  }
}

/** A tool invocation the user can open to see exactly what was asked and returned. */
function ToolCallRow({ activity }: { activity: ToolActivity }) {
  const [open, setOpen] = useState(false);
  const running = activity.status === 'running';
  const argEntries = activity.args ? Object.entries(activity.args) : [];

  return (
    <div className="rounded-md border border-slate-200 bg-white overflow-hidden">
      <button
        onClick={() => !running && setOpen(v => !v)}
        disabled={running}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 text-left text-xs disabled:cursor-default"
      >
        {running ? (
          <div className="w-2.5 h-2.5 border-2 border-indigo-400 border-t-transparent rounded-full animate-spin flex-shrink-0" />
        ) : (
          <Icon name="checkCircle" size={13} className="text-green-600 flex-shrink-0" />
        )}
        <span className="font-medium text-slate-700 font-mono">{activity.tool}</span>
        {argEntries.length > 0 && (
          <span className="text-slate-400 truncate">
            {argEntries.map(([k, v]) => `${k}=${typeof v === 'string' ? v : JSON.stringify(v)}`).join('  ')}
          </span>
        )}
        {!running && (
          <Icon
            name="chevronDown"
            size={13}
            className={`ml-auto flex-shrink-0 text-slate-400 transition-transform ${open ? 'rotate-180' : ''}`}
          />
        )}
      </button>
      {open && !running && activity.result !== undefined && (
        <pre className="px-2.5 pb-2.5 text-[11px] leading-relaxed text-slate-600 whitespace-pre-wrap break-words break-all border-t border-slate-200 pt-2 max-h-56 overflow-y-auto font-mono">
          {stringify(activity.result)}
        </pre>
      )}
    </div>
  );
}

/**
 * Tool activity lives in its own dock above the composer rather than inside the
 * message bubble: it keeps the conversation readable, and the trace stays
 * available after the turn ends instead of vanishing with the next send.
 */
function ToolPanel({ activities }: { activities: ToolActivity[] }) {
  const [open, setOpen] = useState(false);
  const running = activities.some(a => a.status === 'running');

  return (
    <div className="border-t border-slate-100 bg-slate-50 flex-shrink-0">
      <button
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center gap-2 px-4 py-1.5 text-xs text-slate-500 hover:text-slate-700 transition-colors"
      >
        <Icon name="code" size={13} />
        <span>{running ? '正在调用工具...' : `本次对话使用了 ${activities.length} 个工具`}</span>
        <Icon
          name="chevronDown"
          size={13}
          className={`ml-auto transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open && (
        <div className="max-h-64 overflow-y-auto px-3 pb-3 space-y-1.5">
          {activities.map((t, i) => <ToolCallRow key={`${t.tool}-${i}`} activity={t} />)}
          <div className="text-[11px] text-slate-400 pt-0.5">
            写入操作只会生成待审批的计划，需在「整理」页确认后执行。
          </div>
        </div>
      )}
    </div>
  );
}

interface ChatLayoutProps {
  sessions: ChatSession[];
  currentSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => Promise<string>;
  onDeleteSession: (id: string) => void;
  onSendMessage: (content: string) => Promise<void>;
  isLoading: boolean;
  /** Text received so far for the in-flight assistant reply. */
  streamingText?: string;
  /** Live scratchpad from the model's reasoning channel, shown collapsed. */
  /** Tools the agent has invoked during the current turn. */
  toolActivity?: ToolActivity[];
  /** Current agent step, e.g. 2, for the progress hint. */
  currentStep?: number;
  onStop?: () => void;
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

/**
 * Distinguishes "waiting on a tool" from "waiting on the model".
 *
 * Both used to render the same bouncing dots, so a multi-second file read or a
 * slow generation looked identical to a hang. Naming the tool — and its
 * argument — makes it obvious the system is doing something specific.
 */
function RunningIndicator({ step }: { step?: number }) {
  const dots = (
    <div className="flex gap-1 flex-shrink-0">
      <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '0ms' }} />
      <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '150ms' }} />
      <span className="w-1.5 h-1.5 bg-slate-400 rounded-full animate-bounce" style={{ animationDelay: '300ms' }} />
    </div>
  );


  return (
    <div className="flex items-center gap-2 text-slate-500 text-sm">
      {dots}
      <span>{step ? `模型思考中... (第 ${step} 步)` : '模型思考中...'}</span>
    </div>
  );
}

/**
 * One tool call inside the message flow.
 *
 * Deliberately a single line rather than the bordered card used by the dock:
 * the trace should read as a transcript of what ran, not interrupt the answer.
 */
/**
 * One reasoning block inside the transcript. Expanded by default: it arrives
 * interleaved with tool calls, so a collapsed block would leave a bare "思考"
 * row with no indication of what it concluded.
 */
function ReasoningBlock({ text }: { text: string }) {
  if (!text.trim()) return null;
  return (
    <details open className="text-xs text-slate-500 group">
      <summary className="cursor-pointer select-none flex items-center gap-1.5 py-1 hover:text-slate-700 transition-colors">
        <Icon name="chevronRight" size={12} className="transition-transform group-open:rotate-90" />
        思考
        <span className="text-slate-400">({text.length} 字)</span>
      </summary>
      <div className="mt-1.5 pl-3 border-l-2 border-slate-300 leading-relaxed max-h-72 overflow-y-auto">
        <MarkdownText source="model" className="text-xs">{text}</MarkdownText>
      </div>
    </details>
  );
}

function ToolTraceLine({ activity }: { activity: ToolActivity }) {
  const [open, setOpen] = useState(false);
  const running = activity.status === 'running';
  const args = activity.args ? Object.entries(activity.args) : [];

  return (
    <div className="text-xs">
      <button
        onClick={() => !running && setOpen(v => !v)}
        disabled={running}
        className="flex items-center gap-1.5 text-left disabled:cursor-default group"
      >
        {running ? (
          <div className="w-2.5 h-2.5 border-2 border-indigo-400 border-t-transparent rounded-full animate-spin flex-shrink-0" />
        ) : (
          <Icon name="checkCircle" size={12} className="text-green-600 flex-shrink-0" />
        )}
        <span className="font-mono text-indigo-700">{activity.tool}</span>
        {args.length > 0 && (
          <span className="text-slate-500 font-mono truncate">
            {args.map(([k, v]) => `${k}=${typeof v === 'string' ? (v.length > 24 ? v.slice(0, 24) + '…' : v) : JSON.stringify(v)}`).join(' ')}
          </span>
        )}
        {!running && activity.result !== undefined && (
          <Icon
            name="chevronDown"
            size={11}
            className={`flex-shrink-0 text-slate-400 transition-transform ${open ? 'rotate-180' : ''}`}
          />
        )}
      </button>
      {open && !running && activity.result !== undefined && (
        <pre className="mt-1 ml-5 p-2 bg-white/70 rounded text-[11px] leading-relaxed text-slate-600 whitespace-pre-wrap break-words break-words max-h-40 overflow-y-auto">
          {stringify(activity.result)}
        </pre>
      )}
    </div>
  );
}

export function ChatLayout({
  sessions, currentSessionId, onSelectSession, onNewSession,
  onDeleteSession, onSendMessage, isLoading, streamingText, toolActivity,
  currentStep, onStop, emptyState, renderMessage,
}: ChatLayoutProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const currentSession = sessions.find(s => s.id === currentSessionId);

  useEffect(() => { messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [currentSession?.messages]);
  // streamingText changes on every token and the message list is unchanged
  // during a turn, so it needs its own scroll trigger to stay pinned
  useEffect(() => { messagesEndRef.current?.scrollIntoView({ block: 'end' }); }, [streamingText]);
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
  const { collapsed: sessionsCollapsed, toggle: toggleSessions } = useCollapse('sfm.chatSessionsCollapsed');

  return (
    <div className="flex h-full relative">
      {/* Mobile overlay */}
      {sessionsOpen && (
        <div className="fixed inset-0 z-30 bg-black/30 lg:hidden" onClick={() => setSessionsOpen(false)} />
      )}

      {/* Sessions sidebar — desktop. Collapses so it does not stack with the
          app nav rail and squeeze the conversation into a narrow column. */}
      {sessionsCollapsed ? (
        <button
          onClick={toggleSessions}
          title="展开对话列表"
          aria-label="展开对话列表"
          className="hidden lg:flex flex-shrink-0 w-7 items-start justify-center pt-3 text-slate-400 hover:text-indigo-600 transition-colors"
        >
          <Icon name="chevronRight" size={16} />
        </button>
      ) : (
        <div className="hidden lg:flex w-64 bg-white border-r border-slate-200 flex-col flex-shrink-0">
          <SessionList
            sessions={sessions}
            currentSessionId={currentSessionId}
            onNewSession={onNewSession}
            onSelectSession={onSelectSession}
            onDeleteSession={onDeleteSession}
            getTitle={getTitle}
            formatTime={formatTime}
            onToggleCollapse={toggleSessions}
          />
        </div>
      )}

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
                  {msg.activity && msg.activity.length > 0 && (
                    <div className="mb-2 space-y-1.5">
                      {msg.activity.map((a, i) =>
                        a.kind === 'reasoning'
                          ? <ReasoningBlock key={`r${i}`} text={a.text ?? ''} />
                          : <ToolTraceLine key={`t${i}`} activity={a} />
                      )}
                    </div>
                  )}
                  {renderMessage ? renderMessage(msg) : (
                    <MarkdownText
                      source={msg.role === 'user' ? 'plain' : 'model'}
                      className="text-sm leading-relaxed"
                    >
                      {msg.content}
                    </MarkdownText>
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
              <div className="max-w-[90%] sm:max-w-[70%] bg-slate-100 rounded-xl px-4 py-2.5 space-y-2">
                {/* Tool calls sit above the text they produced, so the trace
                    reads in the order it happened instead of being parked in a
                    panel below the conversation. */}
                {toolActivity && toolActivity.length > 0 && (
                  <div className="space-y-1.5">
                    {toolActivity.map((a, i) =>
                      a.kind === 'reasoning'
                        ? <ReasoningBlock key={`r${i}`} text={a.text ?? ''} />
                        : <ToolTraceLine key={`t${i}`} activity={a} />
                    )}
                  </div>
                )}
                {streamingText ? (
                  <div className="text-sm leading-relaxed text-slate-800">
                    <MarkdownText source="model">{streamingText}</MarkdownText>
                    <span className="inline-block w-2 h-4 ml-0.5 bg-slate-400 animate-pulse align-text-bottom" />
                  </div>
                ) : (
                  <RunningIndicator step={currentStep} />
                )}
              </div>
            </div>
          )}
          <div ref={messagesEndRef} />
        </div>

        {/* After the turn the inline trace is gone with the bubble, so keep the
            summary dock for reviewing what the agent did. */}
        {!isLoading && toolActivity && toolActivity.length > 0 && (
          <ToolPanel activities={toolActivity} />
        )}

        <div className="border-t border-slate-100 p-3 flex-shrink-0">
          <form onSubmit={handleSubmit} className="flex items-end gap-2">
            <textarea
              ref={inputRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入问题... (Shift+Enter 换行)"
              rows={1}
              className="flex-1 min-w-0 px-3 py-2 rounded-lg border border-slate-200 text-base sm:text-sm resize-none focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
              style={{ maxHeight: '150px' }}
            />
            {isLoading && onStop ? (
              <button
                type="button"
                onClick={onStop}
                className="flex items-center gap-1.5 px-4 py-2 rounded-lg border border-slate-300 text-slate-700 text-sm font-medium hover:bg-slate-50 transition-colors"
              >
                <span className="w-2.5 h-2.5 rounded-sm bg-current" />
                停止
              </button>
            ) : (
              <button
                type="submit"
                disabled={!input.trim() || isLoading}
                className="flex items-center gap-1.5 px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
              >
                <Icon name="send" size={16} />
                发送
              </button>
            )}
          </form>
        </div>
      </div>
    </div>
  );
}

function SessionList({
  sessions, currentSessionId, onNewSession, onSelectSession, onDeleteSession, getTitle, formatTime, onToggleCollapse,
}: {
  sessions: ChatSession[];
  currentSessionId: string | null;
  onNewSession: () => Promise<string>;
  onSelectSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
  getTitle: (s: ChatSession) => string;
  formatTime: (t: number) => string;
  onToggleCollapse?: () => void;
}) {
  return (
    <>
      <div className="p-3 border-b border-slate-100 flex items-center gap-2">
        <button
          onClick={() => onNewSession()}
          className="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
        >
          <Icon name="plus" size={16} />
          新建对话
        </button>
        {onToggleCollapse && (
          <button
            onClick={onToggleCollapse}
            title="收起对话列表"
            aria-label="收起对话列表"
            className="hidden lg:flex p-1.5 rounded-lg text-slate-400 hover:bg-slate-100 hover:text-indigo-600 transition-colors"
          >
            <Icon name="chevronRight" size={16} className="rotate-180" />
          </button>
        )}
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
              className="absolute right-2 top-1/2 -translate-y-1/2 w-9 h-9 rounded text-slate-400 hover:text-red-500 hover:bg-red-50 opacity-100 sm:opacity-0 sm:group-hover:opacity-100 transition-all flex items-center justify-center"
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
