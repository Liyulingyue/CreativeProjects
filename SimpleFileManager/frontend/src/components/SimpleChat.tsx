import { useState, useRef, useCallback } from 'react';
import { useChatSessions } from '../hooks/useChatSessions';
import { streamAgentMessage } from '../api';
import type { AgentPlan } from '../types';
import { ChatLayout } from './ui/ChatLayout';
import { Icon } from './ui/Icon';

export function SimpleChat() {
  const { sessions, currentSessionId, createSession, selectSession, deleteSession, addMessage } = useChatSessions('agent');
  const [isLoading, setIsLoading] = useState(false);
  const [streamingText, setStreamingText] = useState('');
  const [toolActivity, setToolActivity] = useState<{ tool: string; status: 'running' | 'done' }[]>([]);
  const [currentStep, setCurrentStep] = useState<number | undefined>(undefined);
  const abortRef = useRef<AbortController | null>(null);

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
          } else if (event.type === 'tool') {
            setToolActivity(prev => {
              const next = prev.map(t => (t.status === 'running' ? { ...t, status: 'done' as const } : t));
              return [...next, { tool: event.tool, status: 'running' as const }];
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
        emptyState={
          <div className="h-full flex flex-col items-center justify-center text-slate-400 space-y-3">
            <Icon name="chat" size={40} className="text-slate-300" />
            <div className="text-center">
              <div className="font-medium text-slate-600 mb-1">Agent 对话</div>
              <div className="text-sm">我可以分析文件、搜索内容，并生成待你审批的整理计划</div>
            </div>
          </div>
        }
      />
    </div>
  );
}