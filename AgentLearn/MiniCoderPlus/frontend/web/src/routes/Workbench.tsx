import React, { useState, useRef, useEffect } from 'react';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import 'xterm/css/xterm.css';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { Link } from 'react-router-dom';
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

function Workbench() {
  const queryParams = new URLSearchParams(window.location.search);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [sessionId] = useState(`session_${Math.random().toString(36).substr(2, 9)}`);
  const [workspace, setWorkspace] = useState(queryParams.get('workspace') || '');
  
  // File Explorer State
  const [showExplorer, setShowExplorer] = useState(false);
  const [showFileViewer, setShowFileViewer] = useState(false);
  const [explorerData, setExplorerData] = useState<FileItem[]>([]);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);

  const termRef = useRef<HTMLDivElement>(null);
  const terminalInstance = useRef<Terminal | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

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
    
    // Pass workspace as query param to WebSocket
    const wsUrl = `${protocol}//${backendHost}/ws/terminal/${sessionId}${workspace ? `?workspace=${encodeURIComponent(workspace)}` : ''}`;
    const ws = new WebSocket(wsUrl);
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
      const resp = await fetch('/api/v1/chat?stream=1', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          prompt: input, 
          history: messages,
          session_id: sessionId,
          workspace: workspace || undefined
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

  const handleRefreshChat = () => {
    setMessages([]);
  };

  return (
    <div className="workbench-app">
      <AppHeader
        title="🛠️ MiniCoder Workbench"
        links={[{ to: '/', label: '🔙 Back to Chat' }]}
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
        sessionId={sessionId}
      />

      <PanelGroup direction="horizontal" className="workbench-main" id="workbench-outer">
        {showExplorer && (
          <>
            <Panel defaultSize={20} minSize={10} id="wb-sidebar">
              <div className="file-explorer">
                <div className="explorer-header">
                  <span>FILES</span>
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

        <Panel id="wb-content">
          <PanelGroup direction="horizontal" id="wb-inner">
            {showFileViewer && (
              <>
                <Panel defaultSize={30} minSize={20} id="wb-viewer-panel">
                  <div className="file-viewer">
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

            <Panel defaultSize={showFileViewer ? 40 : 70} minSize={30} id="wb-terminal-area">
              <div className="main-content-area">
                 <div className="terminal-wrapper">
                    <div ref={termRef} className="xterm-container" />
                 </div>
              </div>
            </Panel>
            
            <PanelResizeHandle className="resize-handle" />
            
            <Panel defaultSize={30} minSize={20} id="wb-chat-area">
              <div className="workbench-chat-container">
                <div className="messages-list">
                  {messages.map((msg, i) => (
                    <ChatMessage key={i} msg={msg} />
                  ))}
                  {loading && <LoadingIndicator />}
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
        </Panel>
      </PanelGroup>
    </div>
  );
}

export default Workbench;
