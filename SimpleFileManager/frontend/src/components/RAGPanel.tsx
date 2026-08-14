import { useChatSessions } from '../hooks/useChatSessions';
import { ChatLayout } from './ui/ChatLayout';

interface Source {
  file_path: string;
  score: number;
}

export function RAGPanel() {
  const {
    sessions,
    currentSessionId,
    createSession,
    selectSession,
    deleteSession,
    addMessage,
  } = useChatSessions('rag');

  const handleSendMessage = async (content: string) => {
    let sessionId = currentSessionId;

    if (!sessionId) {
      sessionId = await createSession();
    }

    await addMessage(sessionId, { role: 'user', content });

    try {
      const res = await fetch('/api/rag/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ question: content, top_k: 3 }),
      });

      if (!res.ok) throw new Error('Request failed');

      const data = await res.json();

      const sources: Source[] = data.sources || [];

      await addMessage(sessionId, {
        role: 'assistant',
        content: data.answer || '抱歉，没有找到相关信息。',
        sources,
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
            <div className="text-5xl">📚</div>
            <div className="text-center">
              <div className="font-medium text-slate-600 mb-1">AI 问答</div>
              <div className="text-sm">询问关于文件内容的问题</div>
            </div>
          </div>
        }
      />
    </div>
  );
}
