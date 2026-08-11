import { useState } from 'react';

interface Source {
  file_path: string;
  score: number;
}

interface QAResponse {
  answer: string;
  sources: Source[];
}

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  sources?: Source[];
}

export function RAGPanel() {
  const [question, setQuestion] = useState('');
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!question.trim() || isLoading) return;

    const userMessage: ChatMessage = { role: 'user', content: question };
    setMessages(prev => [...prev, userMessage]);
    setQuestion('');
    setIsLoading(true);

    try {
      const res = await fetch('/api/rag/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ question, top_k: 3 }),
      });
      const data: QAResponse = await res.json();
      const assistantMessage: ChatMessage = {
        role: 'assistant',
        content: data.answer,
        sources: data.sources,
      };
      setMessages(prev => [...prev, assistantMessage]);
    } catch (error) {
      const errorMessage: ChatMessage = {
        role: 'assistant',
        content: '抱歉，发生了错误：' + (error instanceof Error ? error.message : '未知错误'),
      };
      setMessages(prev => [...prev, errorMessage]);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100%',
      background: 'var(--bg-primary)'
    }}>
      <div style={{
        padding: '1rem',
        borderBottom: '1px solid var(--border-color)',
        fontWeight: 600,
        fontSize: '0.9375rem'
      }}>
        AI 问答
      </div>

      <div style={{
        flex: 1,
        overflowY: 'auto',
        padding: '1rem',
        display: 'flex',
        flexDirection: 'column',
        gap: '1rem'
      }}>
        {messages.length === 0 && (
          <div style={{
            textAlign: 'center',
            color: 'var(--text-secondary)',
            padding: '2rem'
          }}>
            询问关于文件内容的问题
          </div>
        )}

        {messages.map((msg, idx) => (
          <div key={idx} style={{
            display: 'flex',
            flexDirection: 'column',
            gap: '0.5rem',
            alignItems: msg.role === 'user' ? 'flex-end' : 'flex-start'
          }}>
            <div style={{
              padding: '0.75rem 1rem',
              borderRadius: 'var(--radius-lg)',
              background: msg.role === 'user' ? 'var(--accent-color)' : 'var(--bg-secondary)',
              color: msg.role === 'user' ? 'white' : 'var(--text-primary)',
              maxWidth: '80%',
              lineHeight: 1.5,
              whiteSpace: 'pre-wrap'
            }}>
              {msg.content}
            </div>

            {msg.sources && msg.sources.length > 0 && (
              <div style={{
                fontSize: '0.75rem',
                color: 'var(--text-secondary)',
                maxWidth: '80%'
              }}>
                <div style={{ marginBottom: '0.25rem', fontWeight: 500 }}>参考文档：</div>
                {msg.sources.map((s, i) => (
                  <div key={i} style={{
                    padding: '0.25rem 0',
                    borderBottom: '1px solid var(--border-color)'
                  }}>
                    <span style={{ color: 'var(--accent-color)' }}>{s.file_path}</span>
                    <span style={{ marginLeft: '0.5rem', opacity: 0.7 }}>
                      相似度: {(s.score * 100).toFixed(1)}%
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}

        {isLoading && (
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
            color: 'var(--text-secondary)'
          }}>
            <div className="loading-spinner" />
            <span>思考中...</span>
          </div>
        )}
      </div>

      <form onSubmit={handleSubmit} style={{
        padding: '1rem',
        borderTop: '1px solid var(--border-color)',
        display: 'flex',
        gap: '0.5rem'
      }}>
        <input
          type="text"
          value={question}
          onChange={e => setQuestion(e.target.value)}
          placeholder="输入你的问题..."
          style={{
            flex: 1,
            padding: '0.625rem 0.875rem',
            borderRadius: 'var(--radius-md)',
            border: '1px solid var(--border-color)',
            background: 'var(--bg-tertiary)',
            color: 'var(--text-primary)',
            fontSize: '0.875rem'
          }}
        />
        <button
          type="submit"
          disabled={!question.trim() || isLoading}
          className="btn btn-primary"
        >
          发送
        </button>
      </form>
    </div>
  );
}
