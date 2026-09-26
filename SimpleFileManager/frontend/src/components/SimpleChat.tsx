import { useChatSessions } from '../hooks/useChatSessions';
import { authFetch } from '../auth';
import { ChatLayout } from './ui/ChatLayout';
import { Icon } from './ui/Icon';

export function SimpleChat() {
  const { sessions, currentSessionId, createSession, selectSession, deleteSession, addMessage } = useChatSessions('agent');

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;
    if (!sessionId) sessionId = await createSession();
    await addMessage(sessionId, { role: 'user', content });
    try {
      const res = await authFetch('/api/agent/chat', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: content }),
      });
      if (!res.ok) throw new Error('Request failed');
      const data = await res.json();
      let reply = data.response || '抱歉，我没有得到回应。';
      const plans = data.plans || [];
      if (plans.length > 0) {
        const actionCount = plans.reduce((n: number, p: { actions?: unknown[] }) => n + (p.actions?.length ?? 0), 0);
        reply += `\n\n已生成 ${plans.length} 份整理计划（共 ${actionCount} 个操作），请到「整理」页的审批中心批准后执行。`;
      }
      await addMessage(sessionId, { role: 'assistant', content: reply });
    } catch (error) {
      await addMessage(sessionId, { role: 'assistant', content: '发生错误：' + (error instanceof Error ? error.message : '未知错误') });
    }
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
        isLoading={false}
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
