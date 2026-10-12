import { useState, useRef, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { streamAgentMessage } from '../api';
import type { AgentPlan } from '../types';
import { useChatSessions } from '../hooks/useChatSessions';
import { ChatLayout } from './ui/ChatLayout';
import { Icon } from './ui/Icon';

const EXAMPLE_PROMPTS = [
  '这个目录里有哪些文件？',
  '找出所有名字带 screenshot 的图片',
  '搜索哪些文件里提到了「合同」',
  '按类型帮我整理文件',
];

interface ToolActivityItem {
  kind: 'reasoning' | 'tool';
  tool?: string;
  status?: 'running' | 'done';
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
  const { sessions, currentSessionId, createSession, selectSession, deleteSession, addMessage } = useChatSessions('agent');
  const [isLoading, setIsLoading] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const reasoningRef = useRef('');
  // kept per session so switching conversations does not erase the trace of
  // what the agent already read on the user's behalf
  const [traces, setTraces] = useState<Record<string, ToolActivityItem[]>>({});
  const tracesRef = useRef<Record<string, ToolActivityItem[]>>({});
  const [currentStep, setCurrentStep] = useState<number | undefined>(undefined);
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
    if (!sessionId) sessionId = await createSession();
    await addMessage(sessionId, { role: 'user', content });

    setIsLoading(true);
    setStreamingText('');
    setCurrentStep(undefined);
    // a fresh turn starts a fresh trace for that session
    updateTrace(sessionId, () => []);

    const controller = new AbortController();
    abortRef.current = controller;

    let reply = '';
    let plans: AgentPlan[] = [];
    let streamError: string | null = null;
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
            // Covers the case where the stream ends without the client having
            // seen every reasoning delta.
            if (event.reasoning && event.reasoning.length > reasoningRef.current.length) {
              reasoningRef.current = event.reasoning;
            }
          } else if (event.type === 'error') {
            streamError = event.message;
          }
        },
        controller.signal
      );
    } catch (e) {
      // An abort is a deliberate stop, not a failure — keep what arrived.
      if (!controller.signal.aborted) {
        streamError = e instanceof Error ? e.message : '请求失败';
      }
    } finally {
      abortRef.current = null;
      updateTrace(sessionId, prev => prev.map(t => (t.status === 'running' ? { ...t, status: 'done' as const } : t)));
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
      finalText += `\n\n已生成 ${plans.length} 份整理计划（共 ${actionCount} 个操作），请到「整理」页的审批中心批准后执行。`;
    }

    // Persist the scratchpad with the reply. It lives only in the streaming
    // bubble otherwise, and vanishes the moment the turn is saved.
    const activity = tracesRef.current[sessionId] ?? [];
    await addMessage(sessionId, {
      role: 'assistant',
      content: finalText,
      reasoning: reasoningRef.current,
      activity,
    });
    setStreamingText('');
  };

  // keeps the ref pointing at the latest closure without re-rendering
  sendRef.current = handleSendMessage;

  return (
    <div className="h-full">
      <ChatLayout
        sessions={sessions}
        currentSessionId={currentSessionId}
        onSelectSession={selectSession}
        onNewSession={createSession}
        onDeleteSession={deleteSession}
        onSendMessage={handleSendMessage}
        isLoading={isLoading}
        streamingText={streamingText}
        toolActivity={toolActivity}
        currentStep={currentStep}
        onStop={handleStop}
        emptyState={<ChatWelcome onPick={(p) => sendRef.current?.(p)} />}
      />
    </div>
  );
}
