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
  tool: string;
  status: 'running' | 'done';
  args?: Record<string, unknown>;
  result?: unknown;
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
  // kept per session so switching conversations does not erase the trace of
  // what the agent already read on the user's behalf
  const [traces, setTraces] = useState<Record<string, ToolActivityItem[]>>({});
  const [currentStep, setCurrentStep] = useState<number | undefined>(undefined);
  const abortRef = useRef<AbortController | null>(null);
  const sendRef = useRef<((content: string) => Promise<void>) | null>(null);

  const toolActivity = currentSessionId ? traces[currentSessionId] : undefined;

  const updateTrace = useCallback((sessionId: string, update: (prev: ToolActivityItem[]) => ToolActivityItem[]) => {
    setTraces(prev => ({ ...prev, [sessionId]: update(prev[sessionId] || []) }));
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

    try {
      await streamAgentMessage(
        content,
        sessionId,
        (event) => {
          if (event.type === 'token') {
            if (!event.text) return;
            reply += event.text;
            setStreamingText(reply);
          } else if (event.type === 'tool_start') {
            updateTrace(sessionId, prev => [...prev, { tool: event.tool, status: 'running', args: event.args }]);
          } else if (event.type === 'tool') {
            updateTrace(sessionId, (prev) => {
              const running = prev.findIndex(t => t.status === 'running');
              if (running === -1) {
                return [...prev, { tool: event.tool, status: 'done', args: event.args, result: event.result }];
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

    await addMessage(sessionId, { role: 'assistant', content: finalText });
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