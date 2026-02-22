import { useState, useRef, useEffect } from 'react';
import { Terminal } from 'xterm';
import { FitAddon } from 'xterm-addon-fit';
import 'xterm/css/xterm.css';
import { PanelGroup } from 'react-resizable-panels';
import { Send, Sparkles } from 'lucide-react';
import type { Message, FileItem } from '../types';
import AppHeader from '../components/AppHeader';
import FileExplorerColumn from '../components/FileExplorerColumn';
import ViewerTerminalStack from '../components/ViewerTerminalStack';
import ChatColumn from '../components/ChatColumn';

function Workbench() {
  const queryParams = new URLSearchParams(window.location.search);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(true);
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
      const endpoint = isStreaming ? '/api/v1/chat?stream=1' : '/api/v1/chat';
      const resp = await fetch(endpoint, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ 
          prompt: input, 
          history: messages,
          session_id: sessionId,
          workspace: workspace || undefined
        }),
      });

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

      <PanelGroup direction="horizontal" className="workbench-main" id="wb-outer-group" autoSaveId="wb-outer-layout">
        {showExplorer && (
          <FileExplorerColumn
            explorerData={explorerData}
            selectedFilePath={selectedFilePath}
            showFileViewer={showFileViewer}
            onRefresh={fetchFileList}
            onSelect={setSelectedFilePath}
            title="FILES"
            panelId="wb-sidebar-panel"
            handleId="wb-handle-sidebar"
          />
        )}

        <ViewerTerminalStack
          showFileViewer={showFileViewer && showExplorer}
          selectedFilePath={selectedFilePath}
          fileContent={fileContent}
          fileLoading={fileLoading}
          onClearSelection={() => setSelectedFilePath(null)}
          termRef={termRef}
          panelId="wb-middle-column"
          panelGroupId="wb-middle-vertical-group"
          autoSaveId="wb-middle-v-layout"
          viewerPanelId="wb-viewer-panel"
          viewerHandleId="wb-handle-viewer-v"
          terminalPanelId="wb-terminal-panel"
        />

        <ChatColumn
          messages={messages}
          loading={loading}
          input={input}
          onInputChange={setInput}
          onSend={handleSend}
          messagesEndRef={messagesEndRef}
          panelId="wb-chat-panel"
          handleId="wb-handle-chat-h"
          emptyState={
            <div className="welcome-message">
              <h1>Welcome to the Workbench</h1>
              <p>Use the sidebar to browse files or send a request below.</p>
              <div className="examples">
                <button onClick={() => setInput("Show me the workspace files")}>🔎 List files</button>
                <button onClick={() => setInput("Open file README.md")}>📄 Open README</button>
              </div>
            </div>
          }
          renderFooter={() => (
            <footer className="footer-area">
              <div className="controls-container">
                <label className="stream-toggle" title="Select whether to stream the agent Response">
                  <input
                    type="checkbox"
                    checked={isStreaming}
                    onChange={(e) => setIsStreaming(e.target.checked)}
                  />
                  <Sparkles size={14} />
                  <span>Streaming Mode</span>
                </label>
              </div>
              <div className="input-container">
                <div className="workbench-input-area">
                  <input
                    type="text"
                    value={input}
                    onChange={(e) => setInput(e.target.value)}
                    onKeyDown={(e) => e.key === 'Enter' && !e.shiftKey && handleSend()}
                    placeholder="Type your request here..."
                    disabled={loading}
                  />
                  <button className="send-btn" onClick={handleSend} disabled={loading || !input.trim()}>
                    {loading ? <div className="typing-indicator" style={{justifyContent: 'center'}}><span style={{backgroundColor: 'white'}}></span><span style={{backgroundColor: 'white'}}></span><span style={{backgroundColor: 'white'}}></span></div> : <Send size={18} />}
                  </button>
                </div>
              </div>
            </footer>
          )}
        />
      </PanelGroup>
    </div>
  );
}

export default Workbench;
