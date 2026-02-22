import React, { useState, useRef, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import '../App.css';
import type { Message } from '../types';
import ChatMessage from '../components/ChatMessage';
import LoadingIndicator from '../components/LoadingIndicator';
import AppHeader from '../components/AppHeader';

interface FileItem {
  name: string;
  path: string;
  abs_path: string;
  type: 'file' | 'directory';
}

function Home() {
  const queryParams = new URLSearchParams(window.location.search);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(true);
  const [workspace, setWorkspace] = useState(queryParams.get('workspace') || '');
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // File Explorer State
  const [showExplorer, setShowExplorer] = useState(false);
  const [showFileViewer, setShowFileViewer] = useState(false);
  const [explorerData, setExplorerData] = useState<FileItem[]>([]);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);

  // Fetch file tree
  const fetchFileList = async () => {
    try {
      const resp = await fetch(`/api/v1/files/list?path=${encodeURIComponent(workspace)}`);
      const data = await resp.json();
      if (data.files) setExplorerData(data.files);
    } catch (e) { console.error('Error fetching file list', e); }
  };

  useEffect(() => {
    if (showExplorer) fetchFileList();
  }, [showExplorer, workspace]);

  // Fetch file content
  useEffect(() => {
    const fetchFile = async () => {
      if (!selectedFilePath) return;
      setFileLoading(true);
      try {
        const resp = await fetch(`/api/v1/files/read?path=${encodeURIComponent(selectedFilePath)}`);
        if (!resp.ok) throw new Error('Not found');
        const data = await resp.json();
        setFileContent(data.content);
      } catch (e) {
        setFileContent('Error loading file.');
      } finally {
        setFileLoading(false);
      }
    };
    fetchFile();
  }, [selectedFilePath]);

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

      const endpoint = isStreaming ? '/api/v1/chat?stream=1' : '/api/v1/chat';
      const resp = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          prompt: input, 
          history: history,
          workspace: workspace || undefined
        }),
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

  const handleRefreshChat = () => {
    setMessages([]);
  };

  return (
    <div className="mini-coder-app">
      <AppHeader
        title="🚀 MiniCoder Plus"
        links={[{ to: '/workbench', label: '🛠️ Try Workbench' }]}
        workspace={workspace}
        onWorkspaceChange={setWorkspace}
        onRefreshChat={handleRefreshChat}
        showExplorer={showExplorer}
        onToggleExplorer={setShowExplorer}
        showFileViewer={showFileViewer}
        onToggleFileViewer={(val) => {
          setShowFileViewer(val);
          if (!val) setSelectedFilePath(null);
        }}
        statusText="Online"
      />

      <PanelGroup direction="horizontal" className="home-main-panels" id="outer-layout">
        {showExplorer && (
          <>
            <Panel defaultSize={20} minSize={10} id="sidebar-panel">
              <div className="file-explorer">
                <div className="explorer-header" style={{ padding: '12px 14px' }}>
                  <span>WORKSPACE FILES</span>
                  <button onClick={fetchFileList} className="refresh-btn">🔄</button>
                </div>
                <div className="explorer-list">
                  {explorerData.map((item, i) => (
                    <div 
                      key={i} 
                      className={`explorer-item ${item.type} ${selectedFilePath === item.abs_path ? 'selected' : ''} ${!showFileViewer && item.type === 'file' ? 'no-peek' : ''}`}
                      onClick={() => {
                        if (item.type === 'file' && showFileViewer) setSelectedFilePath(item.abs_path);
                      }}
                    >
                      {item.type === 'directory' ? '📁' : '📄'} {item.name}
                    </div>
                  ))}
                </div>
              </div>
            </Panel>
            <PanelResizeHandle className="resize-handle" />
          </>
        )}

        <Panel id="content-area">
          <PanelGroup direction="horizontal" id="inner-layout">
            {showFileViewer && (
              <>
                <Panel defaultSize={40} minSize={20} id="viewer-panel">
                  <div className="file-viewer file-viewer-home">
                    {selectedFilePath ? (
                      <>
                        <div className="viewer-header">
                          <span>{selectedFilePath.split(/[/\\]/).pop()}</span>
                          <button onClick={() => setSelectedFilePath(null)} className="close-btn">×</button>
                        </div>
                        <pre className="viewer-content">
                          {fileLoading ? 'Loading...' : fileContent}
                        </pre>
                      </>
                    ) : (
                      <div className="viewer-placeholder">
                        <div className="placeholder-content">
                          <span className="icon">📄</span>
                          <p>Select a file to preview</p>
                        </div>
                      </div>
                    )}
                  </div>
                </Panel>
                <PanelResizeHandle className="resize-handle" />
              </>
            )}

            <Panel minSize={30} id="chat-panel">
              <div className="chat-interface-wrapper">
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
            </Panel>
          </PanelGroup>
        </Panel>
      </PanelGroup>
    </div>
  );
}

export default Home;
