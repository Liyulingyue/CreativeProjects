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
  } = useChatSessions('chat');

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;

    if (!sessionId) {
      sessionId = await createSession();
    }

    await addMessage(sessionId, { role: 'user', content });

    try {
      const res = await fetch('/api/chat/query', {
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
        content: '抱歉，发生了错误：' + (error instanceof Error ? error.message : '未知错误'),
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
            <div className="text-5xl">💬</div>
            <div className="text-center">
              <div className="font-medium text-slate-600 mb-1">单纯对话</div>
              <div className="text-sm">随时问我任何问题，不依赖文档索引</div>
            </div>
          </div>
        }
      />
    </div>
  );
}
