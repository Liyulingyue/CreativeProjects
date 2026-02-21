import React, { useState, useRef, useEffect } from 'react';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import 'xterm/css/xterm.css';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { Link } from 'react-router-dom';
import '../App.css';

interface Message {
  role: 'user' | 'assistant' | 'tool';
  content: string;
  isThought?: boolean;
}

function Workbench() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [sessionId] = useState(`session_${Math.random().toString(36).substr(2, 9)}`);
  
  const termRef = useRef<HTMLDivElement>(null);
  const terminalInstance = useRef<Terminal | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Initialize Terminal
  useEffect(() => {
    if (!termRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      theme: {
        background: '#1e1e1e',
      },
      convertEol: true,
      scrollback: 5000,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    
    // We delay the actual opening to ensure React layout is "settled"
    const initTimer = setTimeout(() => {
        if (!termRef.current) return;
        
        term.open(termRef.current);
        
        let resizeTimeout: any = null;

        const doFit = () => {
            if (termRef.current && termRef.current.offsetWidth > 0 && termRef.current.offsetHeight > 0) {
                try {
                    fitAddon.fit();
                    const cols = term.cols;
                    const rows = term.rows;
                    
                    if (cols > 0 && rows > 0 && ws.readyState === WebSocket.OPEN) {
                        ws.send(JSON.stringify({
                            type: 'resize',
                            cols: cols,
                            rows: rows
                        }));
                    }
                } catch (e) {
                    console.warn("Fit failed, retrying...", e);
                }
            }
        };

        const debouncedFit = () => {
            if (resizeTimeout) clearTimeout(resizeTimeout);
            resizeTimeout = setTimeout(doFit, 100);
        };

        const resizeObserver = new ResizeObserver(() => {
            debouncedFit();
        });
        
        resizeObserver.observe(termRef.current);
        // Initial fit delay to ensure render
        setTimeout(doFit, 200);

        // Save cleanup references locally to avoid closure issues
        savedCleanup = () => {
            resizeObserver.disconnect();
            if (resizeTimeout) clearTimeout(resizeTimeout);
        };
    }, 100);

    let savedCleanup: () => void = () => {};
    terminalInstance.current = term;

    // Connect WebSocket
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const backendPort = "8000"; 
    const backendHost = window.location.hostname === 'localhost' ? `localhost:${backendPort}` : window.location.host;
    const ws = new WebSocket(`${protocol}//${backendHost}/ws/terminal/${sessionId}`);
    wsRef.current = ws;

    ws.onopen = () => {
      console.log("WebSocket connected");
      
      // Immediately sync terminal size upon connection
      if (term.cols > 0 && term.rows > 0) {
        ws.send(JSON.stringify({
          type: 'resize',
          cols: term.cols,
          rows: term.rows
        }));
      }

      const checkAndWrite = () => {
          if (term.element) {
              term.write("\r\n\x1b[32m[Connected to MiniCoder PTY]\x1b[0m\r\n");
          } else {
              setTimeout(checkAndWrite, 100);
          }
      };
      checkAndWrite();
    };

    ws.onmessage = (event) => {
      if (term.element) term.write(event.data);
    };

    ws.onclose = () => {
      console.log("WebSocket closed");
      if (term.element) term.write("\r\n\x1b[31m[Disconnected]\x1b[0m\r\n");
    };

    term.onData((data) => {
      if (!loading && ws.readyState === WebSocket.OPEN) {
        ws.send(data);
      }
    });

    return () => {
      clearTimeout(initTimer);
      savedCleanup();
      try {
          if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
              wsRef.current.close();
          }
      } catch (e) {}
      term.dispose();
      terminalInstance.current = null;
    };
  }, [sessionId]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = async () => {
    if (!input.trim() || loading) return;

    const userMessage: Message = { role: 'user', content: input };
    setMessages((prev) => [...prev, userMessage]);
    setLoading(true);
    setInput('');

    try {
      const resp = await fetch('/chat?stream=1', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          prompt: input, 
          history: messages,
          session_id: sessionId 
        }),
      });

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
            const json = JSON.parse(part.replace('data: ', ''));
            if (json.type === 'update') {
                const item = json.item;
                if (item.role === 'user') continue;
                setMessages(prev => [...prev, {
                    role: item.role,
                    content: item.content || '',
                    isThought: !!(item.role === 'assistant' && item.tool_calls)
                }]);
            } else if (json.type === 'done') {
                setMessages(json.history);
            }
        }
      }
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="workbench-app">
      <header className="app-header">
        <div className="logo-container">🛠️ <span className="logo-text">MiniCoder Workbench</span></div>
        <div className="nav-links">
           <Link to="/" className="nav-btn">🔙 Back to Chat</Link>
        </div>
        <div className="session-info">Session: {sessionId}</div>
      </header>

      <PanelGroup direction="horizontal" className="workbench-main">
        <Panel defaultSize={50} minSize={30}>
          <div className="terminal-wrapper">
             <div ref={termRef} className="xterm-container" />
          </div>
        </Panel>
        
        <PanelResizeHandle className="resize-handle" />
        
        <Panel defaultSize={50} minSize={30}>
          <div className="workbench-chat-container">
            <div className="messages-list">
              {messages.map((msg, i) => (
                <div key={i} className={`message-wrapper ${msg.role} ${msg.isThought ? 'thought' : ''}`}>
                  <div className="message-content">
                    <div className="message-text">{msg.content}</div>
                  </div>
                </div>
              ))}
              <div ref={messagesEndRef} />
            </div>
            
            <footer className="workbench-input-area">
              <input
                type="text"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                onKeyPress={(e) => e.key === 'Enter' && handleSend()}
                placeholder="Ask Agent to do something..."
                disabled={loading}
              />
              <button onClick={handleSend} disabled={loading}>{loading ? '...' : 'Run'}</button>
            </footer>
          </div>
        </Panel>
      </PanelGroup>
    </div>
  );
}

export default Workbench;
