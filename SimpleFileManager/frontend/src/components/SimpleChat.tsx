import { useChatSessions } from '../hooks/useChatSessions';
import { ChatLayout } from './ui/ChatLayout';

export function SimpleChat() {
  const {
    sessions,
    currentSessionId,
    createSession,
    selectSession,
    deleteSession,
    addMessage,
  } = useChatSessions('agent');

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;

    if (!sessionId) {
      sessionId = await createSession();
    }

    await addMessage(sessionId, { role: 'user', content });

    try {
      const res = await fetch('/api/agent/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: content }),
      });

      if (!res.ok) throw new Error('Request failed');

      const data = await res.json();

      await addMessage(sessionId, {
        role: 'assistant',
        content: data.response || '抱歉，我没有得到回应。',
      });
    } catch (error) {
      await addMessage(sessionId, {
        role: 'assistant',
        content: '发生错误：' + (error instanceof Error ? error.message : '未知错误'),
      });
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
          <div className="h-full flex flex-col items-center justify-center text-slate-400 space-y-4">
            <div className="text-5xl">🤖</div>
            <div className="text-center">
              <div className="font-medium text-slate-600 mb-1">Agent 对话</div>
              <div className="text-sm">我可以执行命令、读写文件、搜索内容</div>
            </div>
          </div>
        }
      />
    </div>
  );
}
