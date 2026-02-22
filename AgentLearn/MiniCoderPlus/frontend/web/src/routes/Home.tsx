import { useState, useRef, useEffect } from 'react';
import { PanelGroup } from 'react-resizable-panels';
import { Send, Sparkles } from 'lucide-react';
import type { Message, FileItem } from '../types';
import AppHeader from '../components/AppHeader';
import FileExplorerColumn from '../components/FileExplorerColumn';
import ViewerTerminalStack from '../components/ViewerTerminalStack';
import ChatColumn from '../components/ChatColumn';

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
        feedback: item.feedback
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
        links={[{ to: '/workbench', label: '🛠️ Try Workbench' }]}
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

        {(showFileViewer && showExplorer) && (
          <ViewerTerminalStack
            showFileViewer={showFileViewer && showExplorer}
            selectedFilePath={selectedFilePath}
            fileContent={fileContent}
            fileLoading={fileLoading}
            onClearSelection={() => setSelectedFilePath(null)}
            termRef={viewerTermRef}
            panelId="home-viewer-column"
            panelGroupId="home-viewer-v-group"
            autoSaveId="home-viewer-layout"
            viewerPanelId="home-viewer-panel"
            viewerHandleId="home-handle-viewer"
            terminalPanelId="home-terminal-panel"
            showTerminal={false}
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
    </div>
  );
}

export default Home;
