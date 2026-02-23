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
import Modal from '../components/Modal';

function Home() {
  const createSessionId = () => `session_${Math.random().toString(36).substr(2, 9)}`;
  const queryParams = new URLSearchParams(window.location.search);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [isStreaming, setIsStreaming] = useState(true);
  const [workspace, setWorkspace] = useState(queryParams.get('workspace') || '');
  const [sessionId, setSessionId] = useState(createSessionId());
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const viewerTermRef = useRef<HTMLDivElement>(null);
  const terminalInstance = useRef<Terminal | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  // File Explorer State
  const [showExplorer, setShowExplorer] = useState(false);
  const [showFileViewer, setShowFileViewer] = useState(false);
  const [showTerminal, setShowTerminal] = useState(false);
  const [explorerData, setExplorerData] = useState<FileItem[]>([]);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [showSaveModal, setShowSaveModal] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

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
  const fetchFile = async (path: string) => {
    setFileLoading(true);
    try {
      const resp = await fetch(`/api/v1/files/read?path=${encodeURIComponent(path)}`);
      if (!resp.ok) throw new Error('Not found');
      const data = await resp.json();
      setFileContent(data.content);
    } catch (e) {
      setFileContent('Error loading file.');
    } finally {
      setFileLoading(false);
    }
  };

  useEffect(() => {
    if (!selectedFilePath) return;
    setIsEditing(false); // Reset editing mode on file change
    fetchFile(selectedFilePath);
  }, [selectedFilePath]);

  const handleCancelEdit = () => {
    setIsEditing(false);
    if (selectedFilePath) fetchFile(selectedFilePath);
  };

  const handleSaveFile = async () => {
    if (!selectedFilePath || fileContent === null) return;
    setIsSaving(true);
    try {
      const resp = await fetch('/api/v1/files/write', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          path: selectedFilePath,
          content: fileContent
        })
      });
      if (!resp.ok) throw new Error('Save failed');
      setShowSaveModal(false);
      // Optional: show a toast or success message
    } catch (e) {
      console.error('Error saving file', e);
      alert('Failed to save file');
    } finally {
      setIsSaving(false);
    }
  };

  // Handle Terminal Connection
  useEffect(() => {
    // Only initialize terminal if showTerminal is true
    if (!showTerminal || !viewerTermRef.current) {
        // Cleanup if hidden
        if (wsRef.current) wsRef.current.close();
        if (terminalInstance.current) {
            terminalInstance.current.dispose();
            terminalInstance.current = null;
        }
        return;
    }

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: 'Menlo, Monaco, "Courier New", monospace',
      theme: {
        background: '#1e1e1e',
      },
      convertEol: true,
      scrollback: 5000,
    });
    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    
    // Delay opening to ensure container is ready
    const initTimer = setTimeout(() => {
        if (!viewerTermRef.current) return;
        
        term.open(viewerTermRef.current);
        
        let resizeTimeout: any = null;
        const doFit = () => {
            if (viewerTermRef.current && viewerTermRef.current.offsetWidth > 0 && viewerTermRef.current.offsetHeight > 0) {
                try {
                    fitAddon.fit();
                    const cols = term.cols;
                    const rows = term.rows;
                    if (cols > 0 && rows > 0 && wsRef.current?.readyState === WebSocket.OPEN) {
                        wsRef.current.send(JSON.stringify({ type: 'resize', cols, rows }));
                    }
                } catch (e) {
                    console.warn("Fit failed", e);
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
        
        resizeObserver.observe(viewerTermRef.current);
        setTimeout(doFit, 200);

        savedCleanup = () => {
            resizeObserver.disconnect();
            if (resizeTimeout) clearTimeout(resizeTimeout);
        };
    }, 100);

    let savedCleanup: () => void = () => {};
    terminalInstance.current = term;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const backendPort = "8000"; 
    const backendHost = window.location.hostname === 'localhost' ? `localhost:${backendPort}` : window.location.host;
    const wsUrl = `${protocol}//${backendHost}/ws/terminal/${sessionId}${workspace ? `?workspace=${encodeURIComponent(workspace)}` : ''}`;
    
    const ws = new WebSocket(wsUrl);
    wsRef.current = ws;

    ws.onopen = () => {
      if (term.cols > 0 && term.rows > 0) {
        ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
      }
      term.write("\r\n\x1b[32m[Manual Terminal Session - Session: " + sessionId + "]\x1b[0m\r\n");
    };

    ws.onmessage = (event) => {
      term.write(event.data);
    };

    ws.onclose = () => {
      term.write("\r\n\x1b[31m[Disconnected]\x1b[0m\r\n");
    };

    term.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(data);
      }
    });

    return () => {
      clearTimeout(initTimer);
      savedCleanup();
      if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
        wsRef.current.close();
      }
      term.dispose();
      terminalInstance.current = null;
    };
  }, [showTerminal, sessionId, workspace]);

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
          session_id: sessionId,
          workspace: workspace || undefined,
          // Home route doesn't need terminal (pure LLM chat)
          needs_terminal: false
        }),
      });

      if (!resp.ok) throw new Error('Network response was not ok');

      if (!isStreaming) {
        const data = await resp.json();
        const updated: Message[] = data.history.map((msg: any) => ({
          id: msg.id,
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
                id: item.id,
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
                id: msg.id,
                role: msg.role,
                content: msg.content || '',
                isThought: !!(msg.role === 'assistant' && msg.tool_calls && msg.tool_calls.length > 0),
                tool_calls: msg.tool_calls,
                tool_call_id: msg.tool_call_id,
                name: msg.name,
                feedback: msg.feedback,
                feedback_comment: msg.feedback_comment
              }));
              setMessages(updated)
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

  const loadSessionHistory = async (sessionIdToLoad: string) => {
    if (!sessionIdToLoad) return;
    try {
      const resp = await fetch(`/api/v1/chat/history?session_id=${encodeURIComponent(sessionIdToLoad)}`);
      if (!resp.ok) throw new Error('Failed to load history');
      const data = await resp.json();
      const historyMessages: Message[] = data.items.map((item: any) => ({
        id: item.id,
        role: item.role,
        content: item.content || '',
        isThought: !!(item.role === 'assistant' && item.tool_calls && Object.keys(item.tool_calls || {}).length > 0),
        tool_calls: item.tool_calls,
        tool_call_id: item.tool_call_id,
        name: item.name,
        feedback: item.feedback,
        feedback_comment: item.feedback_comment
      }));
      console.log('[DEBUG] Loaded history messages:', historyMessages);
      setMessages(historyMessages);
      setSessionId(sessionIdToLoad);
      if (data.workspace) {
        setWorkspace(data.workspace);
      }
    } catch (error) {
      console.error('Failed to load session history', error);
    }
  };

  const handleRefreshChat = () => {
    setMessages([]);
    setSessionId(createSessionId());
  };

  const handleSessionDeleted = (deletedId: string) => {
    if (deletedId === sessionId) {
      setMessages([]);
      setSessionId(createSessionId());
    }
  };

  return (
    <div className="mini-coder-app">
      <AppHeader
        title="🚀 MiniCoder Plus"
        links={[
          { to: '/', label: '🏠 首页' },
          { to: '/workbench', label: '🛠️ 工作台' },
          { to: '/feedback-analysis', label: '📊 反馈分析' }
        ]}
        workspace={workspace}
        onWorkspaceChange={setWorkspace}
        onRefreshChat={handleRefreshChat}
        onLoadSession={loadSessionHistory}
        onSessionDeleted={handleSessionDeleted}
        showExplorer={showExplorer}
        onToggleExplorer={setShowExplorer}
        showFileViewer={showFileViewer}
        onToggleFileViewer={(val) => {
          setShowFileViewer(val);
          if (!val) setSelectedFilePath(null);
        }}
        showTerminal={showTerminal}
        onToggleTerminal={setShowTerminal}
        sessionId={sessionId}
      />

      <PanelGroup direction="horizontal" className="home-main-panels" id="home-outer-group" autoSaveId="home-outer-layout">
        {showExplorer && (
          <FileExplorerColumn
            explorerData={explorerData}
            selectedFilePath={selectedFilePath}
            showFileViewer={showFileViewer}
            onRefresh={fetchFileList}
            onSelect={setSelectedFilePath}
            title="WORKSPACE FILES"
            panelId="home-sidebar-panel"
            handleId="home-handle-sidebar"
          />
        )}

        {(showFileViewer || showTerminal) && (
          <ViewerTerminalStack
            showFileViewer={showFileViewer && showExplorer}
            selectedFilePath={selectedFilePath}
            fileContent={fileContent}
            fileLoading={fileLoading}
            isEditing={isEditing}
            onToggleEdit={() => setIsEditing(!isEditing)}
            onCancel={handleCancelEdit}
            onClearSelection={() => setSelectedFilePath(null)}
            onSave={() => setShowSaveModal(true)}
            onContentChange={setFileContent}
            termRef={viewerTermRef}
            panelId="home-viewer-column"
            panelGroupId="home-viewer-v-group"
            autoSaveId="home-viewer-layout"
            viewerPanelId="home-viewer-panel"
            viewerHandleId="home-handle-viewer"
            terminalPanelId="home-terminal-panel"
            showTerminal={showTerminal}
          />
        )}

        <ChatColumn
          messages={messages}
          loading={loading}
          input={input}
          onInputChange={setInput}
          onSend={handleSend}
          messagesEndRef={messagesEndRef}
          sessionId={sessionId}
          onFeedbackSubmit={async (mid, f, c, context) => {
            // Enhanced feedback submission with context snapshot sent as JSON body
            const url = `/api/v1/chat/feedback`;
            const payload = {
              message_id: mid,
              session_id: sessionId,
              feedback: f,
              comment: c,
              context_snapshot: JSON.stringify(context)
            };
            
            try {
              await fetch(url, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload)
              });
            } catch (e) {
              console.error('Failed to save context feedback', e);
            }
          }}
          panelId="home-chat-panel"
          handleId="home-handle-chat"
          showHandle={showFileViewer && showExplorer}
          emptyState={
            <div className="welcome-message">
              <h1>Hello! I'm MiniCoder.</h1>
              <p>Ask me to handle your coding tasks, search the workspace, or explain logic.</p>
              <div className="examples">
                <button onClick={() => setInput("What's in my WorkSpace?")}>🔎 List files</button>
                <button onClick={() => setInput("Create a simple Python script")}>📜 Create script</button>
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

      {showSaveModal && (
        <Modal
          isOpen={showSaveModal}
          title="Save Changes?"
          onClose={() => setShowSaveModal(false)}
          footer={
            <>
              <button 
                className="modal-footer-btn" 
                onClick={() => setShowSaveModal(false)}
                disabled={isSaving}
              >
                Cancel
              </button>
              <button 
                className="modal-footer-btn primary" 
                onClick={handleSaveFile}
                disabled={isSaving}
              >
                {isSaving ? 'Saving...' : 'Confirm Save'}
              </button>
            </>
          }
        >
          <p>Are you sure you want to save changes to <strong>{selectedFilePath?.split(/[/\\]/).pop()}</strong>?</p>
          <p style={{ fontSize: '12px', color: '#64748b', marginTop: '8px' }}>
            This will overwrite the file with the current text content.
          </p>
        </Modal>
      )}
    </div>
  );
}

export default Home;
