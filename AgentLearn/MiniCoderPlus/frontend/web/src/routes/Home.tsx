import React, { useState, useRef, useEffect } from 'react';
import { Link } from 'react-router-dom';
import '../App.css';
import type { Message } from '../types';
import ChatMessage from '../components/ChatMessage';
import LoadingIndicator from '../components/LoadingIndicator';

function Home() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(true);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSend = async () => {
    if (!input.trim() || loading) return;

    const userMessage: Message = { role: 'user', content: input };
    setMessages((prev) => [...prev, userMessage]);
    setInput('');
    setLoading(true);

    try {
      const history = messages.map(m => ({ 
        role: m.role, 
        content: m.content,
        tool_calls: m.tool_calls,
        tool_call_id: m.tool_call_id,
        name: m.name
      }));

      const endpoint = isStreaming ? '/chat?stream=1' : '/chat';
      const resp = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ prompt: input, history: history }),
      });

      if (!resp.ok) throw new Error('Network response was not ok');

      if (!isStreaming) {
        const data = await resp.json();
        const updated: Message[] = data.history.map((msg: any) => ({
          role: msg.role,
          content: msg.content || '',
          isThought: !!(msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0),
          tool_calls: msg.tool_calls,
          tool_call_id: msg.tool_call_id,
          name: msg.name
        }));
        setMessages(updated);
      } else {
        const reader = resp.body!.getReader();
        const decoder = new TextDecoder();
        let buffer = '';

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });

          const parts = buffer.split('\n\n');
          buffer = parts.pop() || '';

          for (const part of parts) {
            if (!part.startsWith('data:')) continue;
            const jsonText = part.replace(/^data:\s*/, '');
            let payload;
            try {
              payload = JSON.parse(jsonText);
            } catch (e) { continue; }

            if (payload.type === 'update') {
              const item = payload.item;
              if (item.role === 'user') continue;
              const isThought = item.role === 'assistant' && item.tool_calls && item.tool_calls.length > 0;
              
              const newMessage: Message = { 
                role: item.role, 
                content: item.content || '', 
                isThought: !!isThought,
                tool_calls: item.tool_calls,
                tool_call_id: item.tool_call_id,
                name: item.name
              };
              setMessages(prev => [...prev, newMessage]);
            } else if (payload.type === 'done') {
              const updated: Message[] = payload.history.map((msg: any) => ({
                role: msg.role,
                content: msg.content || '',
                isThought: !!(msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0),
                tool_calls: msg.tool_calls,
                tool_call_id: msg.tool_call_id,
                name: msg.name
              }));
              setMessages(updated);
            }
          }
        }
      }

    } catch (error) {
      console.error('Stream error:', error);
      setMessages((prev) => [...prev, { role: 'assistant', content: '⚠️ Error: Streaming failed.' }]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mini-coder-app">
      <header className="app-header">
        <div className="logo-container">🚀 <span className="logo-text">MiniCoder Plus</span></div>
        <div className="nav-links">
           <Link to="/workbench" className="nav-btn">🛠️ Try Workbench</Link>
        </div>
        <div className="status">
          <span className="status-dot"></span> Online
        </div>
      </header>

      <main className="chat-container">
        <div className="messages-list">
          {messages.length === 0 && (
            <div className="welcome-message">
              <h1>Hello! I'm MiniCoder.</h1>
              <p>Ask me to handle your coding tasks, search the workspace, or explain logic.</p>
              <div className="examples">
                <button onClick={() => setInput("What's in my WorkSpace?")}>🔎 List files</button>
                <button onClick={() => setInput("Create a simple Python script")}>📜 Create script</button>
              </div>
            </div>
          )}
          {messages.map((msg, index) => (
            <ChatMessage key={index} msg={msg} />
          ))}
          {loading && <LoadingIndicator />}
          <div ref={messagesEndRef} />
        </div>
      </main>

      <footer className="footer-area">
        <div className="controls-container">
          <label className="stream-toggle">
            <input 
              type="checkbox" 
              checked={isStreaming} 
              onChange={(e) => setIsStreaming(e.target.checked)} 
            />
            <span>Streaming Mode</span>
          </label>
        </div>
        <div className="input-container">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyPress={(e) => e.key === 'Enter' && handleSend()}
            placeholder="Type your request here..."
            disabled={loading}
          />
          <button className="send-btn" onClick={handleSend} disabled={loading || !input.trim()}>
            {loading ? '...' : 'Send'}
          </button>
        </div>
      </footer>
    </div>
  );
}

export default Home;
