import { useState, useEffect, useRef } from 'react';
import { Icon } from './Icon';
import { useCollapse } from '../../hooks/useCollapse';
import { MarkdownText } from './MarkdownText';
import { approvePlan, executePlan, rejectPlan, fetchPlan } from '../../api';
import { useToast } from './Toast';
import { ConfirmDialog } from './Dialog';
// Reuse the api layer's types. They were duplicated here, which let the two
// definitions drift apart silently until the session prop stopped type-checking.
import type { ChatMessage, ChatSession } from '../../api';
import type { LLMProvider, AgentPlan } from '../../types';

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
  /**
   * Clears the chat pane to the empty state without persisting anything.
   * The DB row for a new conversation is created lazily on first message
   * send, so this never produces a blank session entry.
   */
  onNewSession: () => void;
  onDeleteSession: (id: string) => void;
  onSendMessage: (content: string) => Promise<void>;
  isLoading: boolean;
  /** Live per-turn header for the streaming bubble. clockMs is set the
   *  moment the user submits so ⏱ renders immediately; ttft_ms / total_ms
   *  / tokens / tps fill in as SSE events arrive. The persisted bubble
   *  reads the same numbers from the message's perf fields. */
  livePerf?: { clockMs: number; ttft_ms?: number; total_ms?: number; tokens?: number; tps?: number };
  /** Resolved provider name for the running turn, surfaced from the
   *  SSE `done` event. Rendered in the live header so the user sees
   *  "via <name>" as soon as the model picks one, without waiting
   *  for the persisted bubble to take over. */
  liveProviderName?: string;
  /** All configured LLM providers. Drives the per-turn dropdown next
   *  to the send button. */
  providers?: LLMProvider[];
  /** Currently-selected provider for the next turn. `null` means
   *  "session default" (which itself falls back to the main on the
   *  server). The dropdown renders the provider name; the server
   *  resolves the actual id. */
  providerId?: string | null;
  onProviderChange?: (id: string | null) => void;
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
 * Wall-clock for the chat footer. formatTime is a relative label that
 * collapses to "刚刚" for anything under a minute, but for a per-turn
 * footer we want the actual clock time so the user can match it against
 * the build logs / the file-mtime of saved sessions.
 */
function formatClock(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString('zh-CN', {
    hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
  });
}

/**
 * Compact, monospaced stat line for assistant messages. Used both in the
 * streaming bubble (live values, missing fields render as "测量中…")
 * and in the persisted bubble (read from msg.ttft_ms / msg.total_ms /
 * msg.tokens / msg.tps). One source of truth keeps the two states
 * pixel-identical once the live set is dropped.
 */
function PerfHeader({
  clockMs,
  perf,
  providerName,
}: {
  clockMs: number;
  perf?: { ttft_ms?: number; total_ms?: number; tokens?: number; tps?: number };
  providerName?: string;
}) {
  const ttft = perf?.ttft_ms ?? 0;
  const total = perf?.total_ms ?? 0;
  const tokens = perf?.tokens ?? 0;
  const tps = perf?.tps ?? 0;
  const hasPerf = total > 0 || ttft > 0 || tokens > 0;
  return (
    <div
      className="pb-1.5 mb-1.5 border-b border-slate-200 text-[11px] text-slate-500 font-mono flex flex-wrap gap-x-3 gap-y-0.5 items-center"
      title="服务器端测得：⏱ 请求开始时刻｜TTFT 首 token 延迟｜共 总耗时｜tokens 生成 token 数｜tok/s 解码吞吐"
    >
      <span>⏱ {formatClock(clockMs)}</span>
      {ttft > 0 && <span>TTFT {ttft}ms</span>}
      {total > 0 && <span>共 {total}ms</span>}
      {tokens > 0 && <span>{tokens} tok</span>}
      {tps > 0 && <span>{tps.toFixed(1)} tok/s</span>}
      {!hasPerf && <span className="opacity-60">测量中…</span>}
      {/* Provider badge. Renders as "via <name>" so the user can tell
          at a glance which model served the reply, and so a re-opened
          session shows the same model that produced it. */}
      {providerName && <span className="text-slate-500" title="由该 provider 生成">via {providerName}</span>}
    </div>
  );
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
        className="flex items-center gap-1.5 text-left disabled:cursor-default group w-full"
      >
        {running ? (
          <div className="w-2.5 h-2.5 border-2 border-indigo-400 border-t-transparent rounded-full animate-spin flex-shrink-0" />
        ) : (
          <Icon name="checkCircle" size={12} className="text-green-600 flex-shrink-0" />
        )}
        <span className="font-mono text-indigo-700 flex-shrink-0">{activity.tool}</span>
        {args.length > 0 && (
          // Show the first arg's value as a short hint (max ~28 chars).
          // Long summaries would otherwise blow past the row width and
          // get hard-truncated by `truncate`, which loses the actual
          // useful context. Full args only render after a click.
          <span className="text-slate-400 font-mono truncate min-w-0">
            {(() => {
              const [k, v] = args[0];
              const value = typeof v === 'string'
                ? (v.length > 28 ? v.slice(0, 28) + '…' : v)
                : JSON.stringify(v);
              return `${k}=${value}${args.length > 1 ? `, +${args.length - 1}` : ''}`;
            })()}
          </span>
        )}
        {!running && (args.length > 0 || activity.result !== undefined) && (
          <Icon
            name="chevronRight"
            size={11}
            className={`flex-shrink-0 text-slate-400 transition-transform ${open ? 'rotate-90' : ''}`}
          />
        )}
      </button>
      {open && !running && (
        <div className="mt-1 ml-5 space-y-2">
          {args.length > 0 && (
            <div className="rounded bg-white/70 p-2 text-[11px] font-mono leading-relaxed">
              {args.map(([k, v]) => (
                <div key={k} className="flex gap-2">
                  <span className="text-slate-500 flex-shrink-0">{k}=</span>
                  <span className="text-slate-700 break-all whitespace-pre-wrap min-w-0">
                    {typeof v === 'string' ? v : JSON.stringify(v, null, 2)}
                  </span>
                </div>
              ))}
            </div>
          )}
          {activity.result !== undefined && (
            <pre className="p-2 bg-white/70 rounded text-[11px] leading-relaxed text-slate-600 whitespace-pre-wrap break-words max-h-40 overflow-y-auto">
              {stringify(activity.result)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Inline approve-and-execute for plans that the agent produced during this
 * turn. Renders a small card under the assistant bubble with the plan title,
 * a status badge, and approve / reject buttons so the user can act without
 * navigating to the approval center.
 *
 * The `plans` snapshot baked into the message is captured at submit time, so
 * its `status` is frozen at 'pending'. On mount we re-read the real status
 * from the server — otherwise a reloaded page would keep offering
 * "批准并执行" for a plan that was already executed, and clicking it would
 * fail with a confusing server error.
 */
function InlinePlanApproval({ plans }: { plans: AgentPlan[] }) {
  const { toast } = useToast();
  const [confirming, setConfirming] = useState<string | null>(null);
  const [rejecting, setRejecting] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [localStatus, setLocalStatus] = useState<Record<string, string>>({});
  const [resolvedIds, setResolvedIds] = useState<Record<string, boolean>>({});
  const [goneIds, setGoneIds] = useState<Record<string, boolean>>({});
  const [errorResult, setErrorResult] = useState<{ plan: AgentPlan; result: AgentPlan } | null>(null);

  const planIds = plans.map(p => p.id).join(',');

  // Re-read the authoritative status for every plan referenced by this
  // bubble. The stored snapshot is stale by design (it records what the
  // agent submitted, not what the user decided).
  useEffect(() => {
    let cancelled = false;
    const ids = planIds.split(',').filter(Boolean);
    if (!ids.length) return;
    Promise.all(ids.map(id => fetchPlan(id).catch(() => null)))
      .then(results => {
        if (cancelled) return;
        const next: Record<string, string> = {};
        const done: Record<string, boolean> = {};
        const gone: Record<string, boolean> = {};
        results.forEach((r, i) => {
          if (r) {
            next[ids[i]] = r.status;
          } else {
            // The server has no such plan. It was deleted from the
            // approval centre, or its id was never real to begin with.
            // Either way there is nothing to approve or reject, and the
            // snapshot's own status is fiction — so drop the buttons
            // instead of offering an action that can only 404.
            gone[ids[i]] = true;
          }
          done[ids[i]] = true;
        });
        setLocalStatus(prev => ({ ...next, ...prev }));
        setResolvedIds(prev => ({ ...done, ...prev }));
        setGoneIds(prev => ({ ...gone, ...prev }));
      });
    return () => { cancelled = true; };
  }, [planIds]);

  const status = (p: AgentPlan) => localStatus[p.id] ?? p.status;

  const onConfirmExecute = async (plan: AgentPlan) => {
    setConfirming(null);
    setBusyId(plan.id);
    try {
      await approvePlan(plan.id);
      const updated = await executePlan(plan.id);
      setLocalStatus(prev => ({ ...prev, [plan.id]: updated.status }));
      if (updated.status === 'executed') {
        toast(`「${plan.title}」已执行`, 'success');
      } else if (updated.status === 'executed_with_errors') {
        setErrorResult({ plan, result: updated });
      } else {
        toast(`执行完成（${updated.status}）`, 'info');
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : '执行失败', 'error');
    } finally {
      setBusyId(null);
    }
  };

  const onConfirmReject = async (plan: AgentPlan) => {
    setRejecting(null);
    setBusyId(plan.id);
    try {
      const updated = await rejectPlan(plan.id);
      setLocalStatus(prev => ({ ...prev, [plan.id]: updated.status }));
      toast(`已拒绝「${plan.title}」`, 'info');
    } catch (e) {
      toast(e instanceof Error ? e.message : '拒绝失败', 'error');
    } finally {
      setBusyId(null);
    }
  };

  const failedActions = errorResult
    ? errorResult.plan.actions.filter((_, i) => {
        const r = errorResult.result.actions?.[i];
        return r && r.status === 'failed';
      })
    : [];

  return (
    <div className="mt-2 pt-2 border-t border-slate-200 space-y-2">
      {plans.map(p => {
        const s = status(p);
        // Gate the action buttons on having resolved the live status.
        // Until the fetch lands we can't know whether this plan is still
        // pending, so rendering a clickable button would let the user
        // fire an action at an already-executed plan.
        const isGone = !!goneIds[p.id];
        const isResolved = !!resolvedIds[p.id];
        const isPending = isResolved && !isGone && s === 'pending';
        const isBusy = busyId === p.id;
        return (
          <div key={p.id} className="rounded-lg bg-white border border-slate-200 p-2.5 text-xs space-y-1.5">
            <div className="flex items-center gap-2 flex-wrap">
              <Icon name="clipboard" size={12} className="text-indigo-500 flex-shrink-0" />
              <span className="font-medium text-slate-700">{p.title}</span>
              <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium ${
                isGone ? 'bg-slate-100 text-slate-400'
                : !isResolved ? 'bg-slate-100 text-slate-400'
                : s === 'pending' ? 'bg-amber-50 text-amber-700'
                : s === 'approved' ? 'bg-blue-50 text-blue-700'
                : s === 'executed' ? 'bg-green-50 text-green-700'
                : s === 'executed_with_errors' ? 'bg-orange-50 text-orange-700'
                : s === 'rejected' ? 'bg-slate-100 text-slate-600'
                : 'bg-slate-100 text-slate-500'
              }`}>
                {isGone ? '计划已删除'
                  : !isResolved ? '…'
                  : s === 'pending' ? '待审批'
                  : s === 'approved' ? '已批准'
                  : s === 'executed' ? '已执行'
                  : s === 'executed_with_errors' ? '部分失败'
                  : s === 'rejected' ? '已拒绝'
                  : s}
              </span>
              <span className="text-slate-400">{p.actions.length} 个操作</span>
            </div>
            {isGone && (
              <p className="text-[11px] text-slate-400 leading-relaxed">
                这个计划在审批中心已被删除，无法再批准或执行。
              </p>
            )}
            {isPending && (
              <div className="flex items-center gap-1.5">
                <button onClick={() => setConfirming(p.id)} disabled={isBusy}
                  className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-green-600 text-white text-xs font-medium hover:bg-green-700 disabled:opacity-50 transition-colors">
                  <Icon name="check" size={12} />批准并执行
                </button>
                <button onClick={() => setRejecting(p.id)} disabled={isBusy}
                  className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-white border border-slate-300 text-slate-600 text-xs font-medium hover:bg-slate-50 hover:text-slate-800 disabled:opacity-50 transition-colors">
                  <Icon name="x" size={12} />拒绝
                </button>
              </div>
            )}
          </div>
        );
      })}
      {confirming && (
        <ConfirmDialog
          open={!!confirming}
          title="执行计划"
          message={`确认执行计划「${plans.find(x => x.id === confirming)?.title ?? ''}」？该操作将真实修改文件系统。`}
          confirmText="执行"
          danger
          onConfirm={() => {
            const plan = plans.find(x => x.id === confirming);
            if (plan) onConfirmExecute(plan);
          }}
          onCancel={() => setConfirming(null)}
        />
      )}
      {rejecting && (
        <ConfirmDialog
          open={!!rejecting}
          title="拒绝计划"
          message={`确认拒绝计划「${plans.find(x => x.id === rejecting)?.title ?? ''}」？拒绝后不会执行任何操作。`}
          confirmText="拒绝"
          danger
          onConfirm={() => {
            const plan = plans.find(x => x.id === rejecting);
            if (plan) onConfirmReject(plan);
          }}
          onCancel={() => setRejecting(null)}
        />
      )}
      {errorResult && (
        <ConfirmDialog
          open={!!errorResult}
          title="部分操作失败"
          message={
            `计划「${errorResult.plan.title}」执行完成，但有 ${failedActions.length} 个操作失败：\n\n` +
            failedActions.map(a => {
              const i = errorResult.plan.actions.findIndex(x => x.id === a.id);
              const r = errorResult.result.actions?.[i];
              return `• ${a.action_type}: ${a.source_path || a.target_path || '未知路径'}\n  错误: ${r?.result || '未知错误'}`;
            }).join('\n\n')
          }
          confirmText="知道了"
          onConfirm={() => setErrorResult(null)}
          onCancel={() => setErrorResult(null)}
        />
      )}
    </div>
  );
}

export function ChatLayout({
  sessions, currentSessionId, onSelectSession, onNewSession,
  onDeleteSession, onSendMessage, isLoading, streamingText, toolActivity,
  currentStep, livePerf, liveProviderName, providers, providerId,
  onProviderChange, onStop, emptyState, renderMessage,
}: ChatLayoutProps) {
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const currentSession = sessions.find(s => s.id === currentSessionId);

  // Look up the provider name for a provider id. Used by the persisted
  // bubble to render "via <name>". Unknown id (legacy rows,
  // deleted providers) falls back to null so the badge is hidden
  // rather than showing a stale name.
  const providerNameFor = (id: string | null | undefined): string | undefined => {
    if (!id || !providers) return undefined;
    return providers.find(p => p.id === id)?.name;
  };

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
                <div className={`max-w-[85%] sm:max-w-[70%] rounded-xl px-4 py-2.5 ${
                  msg.role === 'user'
                    ? 'bg-indigo-600 text-white'
                    : 'bg-slate-100 text-slate-800'
                }`}>
                  {msg.role === 'assistant' && (
                    <PerfHeader
                      clockMs={msg.timestamp}
                      perf={{
                        ttft_ms: msg.ttft_ms,
                        total_ms: msg.total_ms,
                        tokens: msg.tokens,
                        tps: msg.tps,
                      }}
                      // Persisted rows store the resolved provider id;
                      // look the model up in the dropdown list. Legacy
                      // rows from before the providers refactor only
                      // have use_fast=true — show "350M" so the visual
                      // continuity is preserved until the user scrolls
                      // them out of view.
                      providerName={
                        msg.provider_id
                          ? providerNameFor(msg.provider_id)
                          : (msg.use_fast ? 'Fast' : undefined)
                      }
                    />
                  )}
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
                  {msg.plans && msg.plans.length > 0 && (
                    <InlinePlanApproval plans={msg.plans} />
                  )}
                </div>
              </div>
            ))
          )}
          {isLoading && (
            <div className="flex justify-start">
              <div className="max-w-[90%] sm:max-w-[70%] bg-slate-100 rounded-xl px-4 py-2.5 space-y-2">
                {/* Live perf header. Shows the request start clock the
                    moment the user submits and fills in TTFT / total /
                    tokens / tps as the SSE events land, so the user can
                    see the turn is making progress before the first
                    token even arrives. */}
                {livePerf && (
                  <PerfHeader
                    clockMs={livePerf.clockMs}
                    perf={{
                      ttft_ms: livePerf.ttft_ms,
                      total_ms: livePerf.total_ms,
                      tokens: livePerf.tokens,
                      tps: livePerf.tps,
                    }}
                    providerName={liveProviderName}
                  />
                )}
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
          {/* The composer wraps on a phone. Sharing one row with the
              provider picker left the textarea 164px wide on a 390px
              screen — with a placeholder that did not fit — while 120px
              of dead space sat between it and the send button. The
              textarea takes a full row there and the picker and the
              button share the next one; from sm up everything is back on
              a single row as before. */}
          <form onSubmit={handleSubmit} className="flex flex-wrap sm:flex-nowrap items-end gap-2">
            <textarea
              ref={inputRef}
              value={input}
              onChange={e => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="输入问题... (Shift+Enter 换行)"
              rows={1}
              className="w-full sm:w-auto sm:flex-1 min-w-0 px-3 py-2 rounded-lg border border-slate-200 text-base sm:text-sm resize-none focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
              style={{ maxHeight: '150px' }}
            />
            {/* Provider dropdown. Shows provider.name (the friendly alias
                set in /settings) as the option label, with model name as
                subtitle. The __main__ option resolves server-side to the
                singleton is_main=1 row. */}
            {onProviderChange && providers !== undefined && (
              <select
                value={providerId ?? '__main__'}
                onChange={e => onProviderChange(e.target.value === '__main__' ? null : e.target.value)}
                disabled={isLoading}
                title="下一条消息使用的 LLM；选 main 跟随系统默认"
                className="flex-1 sm:flex-none sm:max-w-[220px] px-2 py-2 rounded-lg border border-slate-200 bg-white text-sm text-slate-700 focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 disabled:opacity-40 truncate"
              >
                {(() => {
                  const main = providers.find(p => p.is_main);
                  return (
                    <option value="__main__">
                      {main ? main.name : 'main (默认)'}
                    </option>
                  );
                })()}
                {providers.filter(p => !p.is_main).map(p => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            )}
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
  onNewSession: () => void;
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
