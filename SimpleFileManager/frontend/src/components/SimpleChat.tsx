import { useState, useRef, useCallback, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { streamAgentMessage } from '../api';
import type { AgentPlan } from '../types';
import { useChatSessions } from '../hooks/useChatSessions';
import { useProviders } from '../hooks/useProviders';
import { ChatLayout } from './ui/ChatLayout';
import { Icon } from './ui/Icon';
import { useToast } from './ui/Toast';

const EXAMPLE_PROMPTS = [
  '这个目录里有哪些文件？',
  '找出所有名字带 screenshot 的图片',
  '搜索哪些文件里提到了「合同」',
  '按类型帮我整理文件',
];

interface ToolActivityItem {
  kind: 'reasoning' | 'tool';
  tool?: string;
  status?: 'running' | 'done' | 'cancelled';
  args?: Record<string, unknown>;
  result?: unknown;
  text?: string;
}

/**
 * Empty state stays light: greeting plus example prompts. The tool reference
 * lives on its own page — dumping five tool schemas here buried the
 * conversation behind a wall of English.
 */
function ChatWelcome({ onPick }: { onPick: (prompt: string) => void }) {
  return (
    <div className="h-full flex flex-col items-center justify-center text-slate-400 space-y-4 px-4">
      <Icon name="chat" size={40} className="text-slate-300" />
      <div className="text-center">
        <div className="font-medium text-slate-600 mb-1">Agent 对话</div>
        <div className="text-sm">我可以分析文件、搜索内容，并生成待你审批的整理计划</div>
      </div>

      <div className="flex flex-wrap justify-center gap-1.5 max-w-lg">
        {EXAMPLE_PROMPTS.map(p => (
          <button
            key={p}
            onClick={() => onPick(p)}
            className="px-2.5 py-1.5 rounded-lg bg-white border border-slate-200 text-xs text-slate-600 hover:border-indigo-300 hover:text-indigo-600 transition-colors"
          >
            {p}
          </button>
        ))}
      </div>

      <div className="text-xs text-slate-400">
        我只能读取文件，任何改动都会先生成待你审批的计划。
        <Link to="/agent-tools" className="text-indigo-500 hover:underline ml-1">查看 Agent 能力</Link>
      </div>
    </div>
  );
}

export function SimpleChat() {
  const { toast } = useToast();

  const { sessions, currentSessionId, createSession, startNewSession, selectSession, deleteSession, addMessage } = useChatSessions('agent');
  const providersApi = useProviders();
  // Per-turn provider override. The chat UI shows a dropdown next to
  // the send button; whatever the user picks goes out on the next
  // message as req.provider_id. After the server resolves it, the
  // session's provider_id is updated to the same value, so the next
  // turn defaults to the user's pick even without the override.
  // Switching back to the dropdown's "main" clears the override.
  const [providerId, setProviderId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const reasoningRef = useRef('');
  // kept per session so switching conversations does not erase the trace of
  // what the agent already read on the user's behalf
  const [traces, setTraces] = useState<Record<string, ToolActivityItem[]>>({});
  const tracesRef = useRef<Record<string, ToolActivityItem[]>>({});
  const [currentStep, setCurrentStep] = useState<number | undefined>(undefined);
  // Live per-turn header. clockMs is captured the moment the user submits
  // (so ⏱ renders before the first token); ttft_ms / total_ms / tokens / tps
  // get filled in as the SSE events arrive. Cleared on turn end so the
  // streaming bubble re-uses the same renderer without flicker.
  const [livePerf, setLivePerf] = useState<{ clockMs: number; ttft_ms?: number; total_ms?: number; tokens?: number; tps?: number } | null>(null);
  // Provider name that the running turn actually went through, so
  // the live perf header can show "via <name>" before the bubble
  // gets persisted. Cleared on turn end like livePerf.
  const [liveProviderName, setLiveProviderName] = useState<string | undefined>(undefined);
  const abortRef = useRef<AbortController | null>(null);
  const sendRef = useRef<((content: string) => Promise<void>) | null>(null);

  const toolActivity = currentSessionId ? traces[currentSessionId] : undefined;

  const updateTrace = useCallback((sessionId: string, update: (prev: ToolActivityItem[]) => ToolActivityItem[]) => {
    // Keep a ref alongside the state: the transcript has to be readable
    // synchronously when the turn is saved, and state would still be stale
    // inside the same tick the stream finished.
    const next = update(tracesRef.current[sessionId] || []);
    tracesRef.current = { ...tracesRef.current, [sessionId]: next };
    setTraces(prev => ({ ...prev, [sessionId]: next }));
  }, []);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
  }, []);

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;
    if (!sessionId) {
      try {
        sessionId = await createSession();
      } catch (e) {
        toast(e instanceof Error ? e.message : '创建会话失败', 'error');
        return;
      }
    }
    await addMessage(sessionId, { role: 'user', content });

    setIsLoading(true);
    setStreamingText('');
    setCurrentStep(undefined);
    // a fresh turn starts a fresh trace for that session
    updateTrace(sessionId, () => []);

    // Anchor the live perf header to the moment the user pressed send. The
    // clock is rendered from this immediately, before any token arrives,
    // so the user sees the request start time without waiting on the
    // model. Subsequent fields fill in as events come in.
    const clockMs = Date.now();
    setLivePerf({ clockMs });

    const controller = new AbortController();
    abortRef.current = controller;

    let reply = '';
    let plans: AgentPlan[] = [];
    let streamError: string | null = null;
    let turnPerf: { ttft_ms: number; total_ms: number; tokens: number; tps: number } = {
      ttft_ms: 0, total_ms: 0, tokens: 0, tps: 0,
    };
    // The server is the source of truth for which provider actually
    // served the reply. We capture it from the SSE `done` event and
    // mirror it into the persisted message so the bubble's
    // "via <name>" badge survives a reload.
    let servedProviderId: string | null = providerId;
    // Stash the name on the component so the live perf header can
    // render "via <name>" as soon as the SSE `done` lands, before
    // the persisted bubble takes over. Cleared on turn end.
    setLiveProviderName(undefined);
    // Client-side anchors for the live header. Used only when the server
    // doesn't emit the perf fields (e.g. legacy build) — and to measure
    // TTFT for the streaming bubble the moment the first text delta
    // lands (reasoning chunk or answer chunk — whichever arrives first),
    // before the server's "done" event arrives. Mirrors the backend's
    // "first LLM text byte" semantics so the streaming bubble's TTFT
    // agrees with the persisted message's TTFT.
    const clientT0 = performance.now();
    let clientFirstTokenAt: number | null = null;
    reasoningRef.current = '';

    try {
      await streamAgentMessage(
        content,
        sessionId,
        (event) => {
          if (event.type === 'reasoning') {
            // One ordered stream, not two: the model reasons, calls a tool,
            // reasons again, calls another tool. Merging them per-arrival is
            // what makes the transcript read like what actually happened
            // instead of "all thinking, then all tool calls, then the answer".
            // Consecutive reasoning deltas fold into the previous block so a
            // step does not fragment into dozens of one-word entries.
            reasoningRef.current += event.text ?? '';
            updateTrace(sessionId, (prev) => {
              const last = prev[prev.length - 1];
              if (last?.kind === 'reasoning') {
                return [...prev.slice(0, -1), { ...last, text: last.text + (event.text ?? '') }];
              }
              return [...prev, { kind: 'reasoning', text: event.text ?? '' }];
            });
          } else if (event.type === 'token') {
            if (!event.text) return;
            reply += event.text;
            setStreamingText(reply);
            // First token: stamp TTFT and reveal it in the live header.
            // The server's measurement is more accurate and overwrites
            // this on the "done" event, but the client value lets the
            // streaming bubble show TTFT the moment the first character
            // lands, without waiting for the whole turn to finish.
            if (clientFirstTokenAt === null) {
              clientFirstTokenAt = performance.now();
              const ttft = Math.round(clientFirstTokenAt - clientT0);
              setLivePerf(prev => prev ? { ...prev, ttft_ms: ttft } : prev);
            }
          } else if (event.type === 'tool_start') {
            updateTrace(sessionId, prev => [...prev, { kind: 'tool', tool: event.tool, status: 'running', args: event.args }]);
          } else if (event.type === 'tool') {
            updateTrace(sessionId, (prev) => {
              // findLastIndex needs ES2023; reverse scan keeps the compiler
              // target unchanged. Matching the *last* running entry matters now
              // that reasoning entries are interleaved with tool calls.
              let running = -1;
              for (let i = prev.length - 1; i >= 0; i--) {
                if (prev[i].status === 'running') { running = i; break; }
              }
              if (running === -1) {
                return [...prev, { kind: 'tool', tool: event.tool, status: 'done', args: event.args, result: event.result }];
              }
              const next = prev.slice();
              next[running] = {
                ...next[running],
                status: 'done',
                args: event.args ?? next[running].args,
                result: event.result,
              };
              return next;
            });
          } else if (event.type === 'step') {
            setCurrentStep(event.step);
          } else if (event.type === 'done') {
            plans = event.plans || [];
            reply = event.response || reply;
            // Server echoes the resolved provider. Capture for the
            // persisted bubble and for the session-row update below.
            // The live badge wants the model name (not the operator-
            // given handle), so look it up in the providers list —
            // if the providers are still loading the user just sees
            // the model field on the next turn.
            if (event.provider_id) servedProviderId = event.provider_id;
            if (event.provider_name) {
              const match = providersApi.providers.find(p => p.id === event.provider_id);
              setLiveProviderName(match?.model || event.provider_name);
            }
            // The server emits the authoritative perf numbers in the
            // "done" event. Mirror them into both turnPerf (used when
            // persisting the assistant message) and livePerf (kept
            // visible until the persisted bubble takes its place).
            if (typeof event.ttft_ms === 'number') {
              turnPerf = {
                ttft_ms: event.ttft_ms,
                total_ms: event.total_ms ?? 0,
                tokens: event.tokens ?? 0,
                tps: event.tps ?? 0,
              };
              setLivePerf(prev => prev ? {
                ...prev,
                ttft_ms: turnPerf.ttft_ms,
                total_ms: turnPerf.total_ms,
                tokens: turnPerf.tokens,
                tps: turnPerf.tps,
              } : prev);
            }
            // Covers the case where the stream ends without the client having
            // seen every reasoning delta.
            if (event.reasoning && event.reasoning.length > reasoningRef.current.length) {
              reasoningRef.current = event.reasoning;
            }
          } else if (event.type === 'error') {
            streamError = event.message;
          }
        },
        controller.signal,
        { providerId: providerId }
      );
    } catch (e) {
      // An abort is a deliberate stop, not a failure — keep what arrived.
      if (!controller.signal.aborted) {
        streamError = e instanceof Error ? e.message : '请求失败';
      }
    } finally {
      abortRef.current = null;
      updateTrace(sessionId, prev => prev.map(t => (t.status === 'running' ? { ...t, status: 'cancelled' as const } : t)));
      setIsLoading(false);
      setCurrentStep(undefined);
    }

    let finalText = reply.trim();
    if (!finalText) {
      finalText = streamError
        ? '发生错误：' + streamError
        : (controller.signal.aborted ? '已停止生成。' : '抱歉，我没有得到回应。');
    } else if (streamError) {
      finalText += `\n\n（生成中断：${streamError}）`;
    }

    if (plans.length > 0) {
      const actionCount = plans.reduce((n: number, p: AgentPlan) => n + (p.actions?.length ?? 0), 0);
      finalText += `\n\n已生成 ${plans.length} 份整理计划（共 ${actionCount} 个操作），请到「整理」页审批中心批准后执行。`;
    }

    // Persist the scratchpad with the reply. It lives only in the streaming
    // bubble otherwise, and vanishes the moment the turn is saved.
    const activity = tracesRef.current[sessionId] ?? [];
    await addMessage(sessionId, {
      role: 'assistant',
      content: finalText,
      reasoning: reasoningRef.current,
      activity,
      ttft_ms: turnPerf.ttft_ms,
      total_ms: turnPerf.total_ms,
      tokens: turnPerf.tokens,
      tps: turnPerf.tps,
      // The server is the source of truth for which provider
      // actually served the reply; the bubble looks the name up
      // from the providers list and renders "via <name>".
      provider_id: servedProviderId,
      // Plans the agent submitted during this turn. The chat bubble
      // renders an inline "批准并执行" button per plan so the user
      // can act on a one-turn plan without leaving the conversation.
      plans,
    });
    setStreamingText('');
    // Drop the live header once the persisted bubble (which carries the
    // same numbers via the message's perf fields) takes its place.
    setLivePerf(null);
    setLiveProviderName(undefined);
    // Refresh sessions so the new assistant message appears in the
    // sidebar with the resolved provider_id — without this the row
    // would still show the previous (pre-override) state until the
    // user clicks away and back.
    await providersApi.refresh();
  };

  // keeps the ref pointing at the latest closure without re-rendering
  sendRef.current = handleSendMessage;

  // "New conversation" wrapper. The new session's default provider is
  // already set to the main on the server; sync the dropdown to match
  // so the user sees what the session will actually use.
  const handleNewSession = () => {
    startNewSession();
    setProviderId(null);
  };

  // When the user clicks an existing session, sync the dropdown to
  // whatever the session was last used with. Sessions created before
  // the providers refactor have provider_id=null, in which case we
  // show the dropdown's neutral "main" placeholder.
  useEffect(() => {
    if (!currentSessionId) {
      setProviderId(null);
      return;
    }
    const session = sessions.find(s => s.id === currentSessionId);
    setProviderId(session?.provider_id ?? null);
  }, [currentSessionId, sessions]);

  // Snap to the main provider once on first load so the empty state
  // shows the actual model name. A deliberate user choice (null) is
  // never overridden.
  useEffect(() => {
    if (providerId === null && providersApi.main && !currentSessionId) {
      setProviderId(providersApi.main.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="h-full">
      <ChatLayout
        sessions={sessions}
        currentSessionId={currentSessionId}
        onSelectSession={selectSession}
        onNewSession={handleNewSession}
        onDeleteSession={deleteSession}
        onSendMessage={handleSendMessage}
        isLoading={isLoading}
        streamingText={streamingText}
        toolActivity={toolActivity}
        currentStep={currentStep}
        livePerf={livePerf ?? undefined}
        liveProviderName={liveProviderName}
        providers={providersApi.providers}
        providerId={providerId}
        onProviderChange={setProviderId}
        onStop={handleStop}
        emptyState={<ChatWelcome onPick={(p) => sendRef.current?.(p)} />}
      />
    </div>
  );
}
