import { useEffect, useState, useRef, useCallback } from 'react';
import { streamAgentMessage, getAgentTools } from '../api';
import type { AgentToolDefinition } from '../api';
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
 * What the agent is actually allowed to do. The definitions come from
 * /agent/tools, so this stays truthful when the backend gains or drops a tool
 * instead of duplicating the list here.
 */
function AgentCapabilities({ onPick }: { onPick: (prompt: string) => void }) {
  const [tools, setTools] = useState<AgentToolDefinition[] | null>(null);

  useEffect(() => {
    getAgentTools()
      .then(d => setTools(d.definitions || []))
      .catch(() => setTools([]));
  }, []);

  return (
    <div className="w-full max-w-2xl px-4 py-6">
      <div className="text-center mb-6">
        <div className="text-base font-medium text-slate-700 mb-1">Agent 对话</div>
        <div className="text-sm text-slate-400">我可以读取和分析你的文件，生成待你审批的整理计划</div>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-5">
        {(tools || []).map(t => (
          <div key={t.function.name} className="flex items-start gap-2 p-2.5 bg-white rounded-lg border border-slate-200">
            <Icon name="code" size={14} className="text-slate-400 mt-0.5 flex-shrink-0" />
            <div className="min-w-0">
              <div className="text-xs font-medium text-slate-700 font-mono">{t.function.name}</div>
              <div className="text-xs text-slate-400 leading-relaxed">{t.function.description}</div>
              {t.function.parameters?.properties && (
                <div className="mt-1 flex flex-wrap gap-1">
                  {Object.entries(t.function.parameters.properties).map(([name, spec]) => (
                    <span key={name} className="text-[10px] px-1.5 py-0.5 rounded bg-slate-100 text-slate-500 font-mono">
                      {name}: {spec.type}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        ))}
        {tools === null && <div className="col-span-full text-center text-sm text-slate-400">加载能力清单...</div>}
      </div>

      <div className="text-xs text-slate-400 mb-2">试试这样问：</div>
      <div className="flex flex-wrap gap-1.5">
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

      <div className="mt-5 text-xs text-slate-400 leading-relaxed">
        所有写入操作都只生成待审批的计划，需要你在「整理」页确认后才会执行。
      </div>
    </div>
  );
}

export function SimpleChat() {
  const { sessions, currentSessionId, createSession, selectSession, deleteSession, addMessage } = useChatSessions('agent');
  const [isLoading, setIsLoading] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const [toolActivity, setToolActivity] = useState<ToolActivityItem[]>([]);
  const [currentStep, setCurrentStep] = useState<number | undefined>(undefined);
  const abortRef = useRef<AbortController | null>(null);
  const sendRef = useRef<((content: string) => Promise<void>) | null>(null);

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
    setToolActivity([]);
    setCurrentStep(undefined);

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
            setToolActivity(prev => [
              ...prev,
              { tool: event.tool, status: 'running', args: event.args },
            ]);
          } else if (event.type === 'tool') {
            // fill in the running entry started by tool_start, else append
            setToolActivity(prev => {
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
      setToolActivity(prev => prev.map(t => ({ ...t, status: 'done' as const })));
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
        emptyState={<AgentCapabilities onPick={(p) => sendRef.current?.(p)} />}
      />
    </div>
  );
}