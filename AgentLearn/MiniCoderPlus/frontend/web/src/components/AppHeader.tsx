import React from 'react';
import { Link } from 'react-router-dom';

export interface LinkItem {
  to: string;
  label: string;
}

interface AppHeaderProps {
  title: string;                     // can contain emoji/text
  links: LinkItem[];                // navigation links on left side
  workspace: string;                // current workspace value
  onWorkspaceChange: (ws: string) => void;
  onRefreshChat?: () => void;       // optional: callback to clear/refresh chat
  showExplorer?: boolean;           // current explorer state
  onToggleExplorer?: (val: boolean) => void;
  showFileViewer?: boolean;         // current file viewer state
  onToggleFileViewer?: (val: boolean) => void;
  onLoadSession?: (sessionId: string) => void;
  onSessionDeleted?: (sessionId: string) => void;
  sessionId?: string;               // optional session display
  statusText?: string;              // optional status, e.g. "Online"
}

type SessionSummary = {
  session_id: string;
  workspace?: string;
  last_activity: string;
  good_count?: number;  // Number of 👍 feedbacks
  bad_count?: number;   // Number of 👎 feedbacks
};

const AppHeader: React.FC<AppHeaderProps> = ({
  title,
  links,
  workspace,
  onWorkspaceChange,
  onRefreshChat,
  showExplorer,
  onToggleExplorer,
  showFileViewer,
  onToggleFileViewer,
  onLoadSession,
  onSessionDeleted,
  sessionId,
  statusText,
}) => {
  const [absolutePath, setAbsolutePath] = React.useState<string>('');
  const [inputValue, setInputValue] = React.useState<string>(workspace);
  const [showPopover, setShowPopover] = React.useState<boolean>(false);
  const [showClearConfirm, setShowClearConfirm] = React.useState<boolean>(false);
  const [showFilesPopover, setShowFilesPopover] = React.useState<boolean>(false);
  const [showHistoryPopover, setShowHistoryPopover] = React.useState<boolean>(false);
  const [sessionList, setSessionList] = React.useState<SessionSummary[]>([]);
  
  const popoverRef = React.useRef<HTMLDivElement>(null);
  const clearConfirmRef = React.useRef<HTMLDivElement>(null);
  const filesPopoverRef = React.useRef<HTMLDivElement>(null);
  const historyPopoverRef = React.useRef<HTMLDivElement>(null);

  // keep internal input synced when parent workspace prop changes
  React.useEffect(() => {
    setInputValue(workspace);
  }, [workspace]);

  const resolveAbsolute = React.useCallback(async (pathToResolve: string) => {
    try {
      const params = new URLSearchParams();
      if (pathToResolve) params.append('path', pathToResolve);
      const resp = await fetch(`/api/v1/workspace/resolve?${params.toString()}`);
      if (!resp.ok) {
        setAbsolutePath('Error resolving path');
        return;
      }
      const data = await resp.json();
      setAbsolutePath(data.absolute);
    } catch (error) {
      console.error('Workspace resolve failed', error);
      setAbsolutePath('Server unreachable');
    }
  }, []);

  React.useEffect(() => {
    resolveAbsolute(workspace);
  }, [workspace, resolveAbsolute]);

  const fetchSessions = React.useCallback(async () => {
    try {
      const resp = await fetch('/api/v1/chat/sessions');
      if (resp.ok) {
        const data = await resp.json();
        setSessionList(data);
      }
    } catch (e) { console.error('Error fetching sessions', e); }
  }, []);

  const formatTimestamp = React.useCallback((value?: string) => {
    if (!value) return '未知时间';
    const parsed = Date.parse(value);
    if (Number.isNaN(parsed)) return value;
    return new Date(parsed).toLocaleString();
  }, []);

  const deleteSession = React.useCallback(async (sessionToDelete: string) => {
    try {
      const resp = await fetch(`/api/v1/chat/history?session_id=${encodeURIComponent(sessionToDelete)}`, { method: 'DELETE' });
      if (resp.ok) {
        fetchSessions();
        onSessionDeleted?.(sessionToDelete);
      }
    } catch (error) {
      console.error('Failed to delete session', error);
    }
  }, [fetchSessions, onSessionDeleted]);

  React.useEffect(() => {
    if (showHistoryPopover) fetchSessions();
  }, [showHistoryPopover, fetchSessions]);

  // handle outside click to close popovers
  React.useEffect(() => {
    const handleOutsideClick = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        setShowPopover(false);
      }
      if (clearConfirmRef.current && !clearConfirmRef.current.contains(e.target as Node)) {
        setShowClearConfirm(false);
      }
      if (filesPopoverRef.current && !filesPopoverRef.current.contains(e.target as Node)) {
        setShowFilesPopover(false);
      }
      if (historyPopoverRef.current && !historyPopoverRef.current.contains(e.target as Node)) {
        setShowHistoryPopover(false);
      }
    };
    if (showPopover || showClearConfirm || showFilesPopover || showHistoryPopover) {
      document.addEventListener('mousedown', handleOutsideClick);
    }
    return () => document.removeEventListener('mousedown', handleOutsideClick);
  }, [showPopover, showClearConfirm, showFilesPopover, showHistoryPopover]);

  const handleUpdate = () => {
    onWorkspaceChange(inputValue);
    resolveAbsolute(inputValue);
    setShowPopover(false);
  };

  return (
    <header className="app-header">
      <div className="logo-container">{title}</div>
      <div className="nav-links">
        {links.map((ln, idx) => (
          <Link key={idx} to={ln.to} className="nav-btn">
            {ln.label}
          </Link>
        ))}
      </div>
      <div className="session-info" style={{ position: 'relative' }} ref={popoverRef}>
        {onToggleExplorer && (
          <div ref={filesPopoverRef} style={{ position: 'relative', display: 'inline-block' }}>
            <button 
              className={`workspace-trigger-btn ${(showExplorer || showFilesPopover) ? 'active' : ''}`}
              onClick={() => setShowFilesPopover(!showFilesPopover)}
              style={{ marginRight: '8px' }}
              title="File viewing settings"
            >
              📂 Files Tool
            </button>
            {showFilesPopover && (
              <div className="workspace-popover" style={{ width: '220px', right: '4px' }}>
                <h4>📂 Files Settings</h4>
                <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', marginTop: '4px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer', fontSize: '13px' }}>
                    <input 
                      type="checkbox" 
                      checked={!!showExplorer} 
                      onChange={(e) => onToggleExplorer!(e.target.checked)} 
                    />
                    Enable File Tree
                  </label>
                  <label style={{ 
                    display: 'flex', 
                    alignItems: 'center', 
                    gap: '8px', 
                    cursor: showExplorer ? 'pointer' : 'not-allowed', 
                    fontSize: '13px',
                    opacity: showExplorer ? 1 : 0.5 
                  }}>
                    <input 
                      type="checkbox" 
                      disabled={!showExplorer}
                      checked={!!showFileViewer} 
                      onChange={(e) => onToggleFileViewer!(e.target.checked)} 
                    />
                    Enable File Preview
                  </label>
                </div>
              </div>
            )}
          </div>
        )}
        {onRefreshChat && (
          <div ref={clearConfirmRef} style={{ position: 'relative', display: 'inline-block' }}>
            <button 
              className={`workspace-trigger-btn ${showClearConfirm ? 'active' : ''}`}
              onClick={() => setShowClearConfirm(!showClearConfirm)}
              title="Clear current chat history"
            >
              🧹 Clear Chat
            </button>
            {showClearConfirm && (
              <div className="workspace-popover" style={{ width: '240px', right: '4px' }}>
                <h4>🧹 Clear Chat?</h4>
                <p style={{ fontSize: '12px', color: '#6b7280', margin: '4px 0 12px 0' }}>
                  This will remove all current messages.
                </p>
                <div className="workspace-popover-actions">
                  <button 
                    className="workspace-popover-btn" 
                    onClick={() => setShowClearConfirm(false)}
                      style={{ padding: '4px 8px' }}
                  >
                    Cancel
                  </button>
                  <button 
                    className="workspace-popover-btn primary" 
                    style={{ backgroundColor: '#ef4444', borderColor: '#ef4444', padding: '4px 8px' }}
                    onClick={() => {
                      onRefreshChat();
                      setShowClearConfirm(false);
                    }}
                  >
                    Confirm
                  </button>
                </div>
              </div>
            )}
          </div>
        )}
        {onLoadSession && (
          <div ref={historyPopoverRef} style={{ position: 'relative', display: 'inline-block' }}>
            <button
              className={`workspace-trigger-btn ${showHistoryPopover ? 'active' : ''}`}
              onClick={() => setShowHistoryPopover(!showHistoryPopover)}
              title="Load a saved session"
            >
              📜 历史
            </button>
            {showHistoryPopover && (
              <div className="workspace-popover" style={{ width: '320px', right: '4px' }}>
                <h4>📜 Session History</h4>
                {sessionList.length === 0 ? (
                  <p style={{ fontSize: '12px', color: '#6b7280', margin: '6px 0' }}>
                    暂无历史会话。
                  </p>
                ) : (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: '6px', maxHeight: '260px', overflowY: 'auto', marginTop: '6px' }}>
                    {sessionList.map((session) => (
                      <div key={session.session_id} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                        <button
                          className={`workspace-popover-btn ${sessionId === session.session_id ? 'primary' : ''}`}
                          style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'flex-start', padding: '6px 8px', textAlign: 'left', gap: '4px' }}
                          onClick={() => {
                            onLoadSession(session.session_id);
                            setShowHistoryPopover(false);
                          }}
                        >
                          <span style={{ fontSize: '13px', fontWeight: 600, width: '100%', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
                            {session.session_id}
                          </span>
                          <span style={{ fontSize: '11px', color: '#6b7280' }}>
                            {session.workspace ? `${session.workspace} · ` : ''}
                            {formatTimestamp(session.last_activity)}
                          </span>
                          {(session.good_count !== undefined || session.bad_count !== undefined) && (
                            (session.good_count || 0) + (session.bad_count || 0) > 0 && (
                              <span style={{ fontSize: '11px', color: '#059669', marginTop: '2px' }}>
                                👍 {session.good_count || 0} &nbsp; 👎 {session.bad_count || 0}
                              </span>
                            )
                          )}
                        </button>
                        <button
                          className="workspace-popover-btn"
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteSession(session.session_id);
                          }}
                          style={{
                            background: '#fee2e2',
                            border: '1px solid #fecaca',
                            color: '#ef4444',
                            padding: '4px 8px',
                            borderRadius: '4px',
                            minWidth: '56px',
                            fontSize: '11px',
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            transition: 'background 0.2s'
                          }}
                          onMouseEnter={(e) => (e.currentTarget.style.background = '#fecaca')}
                          onMouseLeave={(e) => (e.currentTarget.style.background = '#fee2e2')}
                        >
                          🗑️ 删除
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        )}
        &nbsp;
        <div 
          className={`workspace-trigger-btn ${showPopover ? 'active' : ''}`}
          onClick={() => setShowPopover(!showPopover)}
        >
          📂 {workspace || '工作区'}
        </div>

        {showPopover && (
          <div className="workspace-popover">
            <h4>📂 Workspace Settings</h4>
            
            <div className="abs-path-display">
              <div style={{fontWeight: 'bold', marginBottom: '4px'}}>Absolute Path:</div>
              {absolutePath || 'Resolving...'}
            </div>

            <input
              type="text"
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value)}
              placeholder="e.g. WorkSpace, /tmp, or none"
              className="workspace-popover-input"
              autoFocus
            />

            <div className="workspace-popover-actions">
              <button
                className="workspace-popover-btn"
                onClick={() => {
                  // reset to default workspace name without changing absolute path
                  const defaultName = 'WorkSpace';
                  setInputValue(defaultName);
                }}
              >
                🔁 Reset
              </button>
              <button
                className="workspace-popover-btn primary"
                onClick={handleUpdate}
              >
                ✏️ Apply
              </button>
            </div>
          </div>
        )}

        {sessionId && (
          <> &nbsp; Session:&nbsp;<code style={{ marginLeft: '4px' }}>{sessionId}</code></>
        )}
        {statusText && !sessionId && (
          <> &nbsp; <span className="status-dot"></span> {statusText}</>
        )}
      </div>
    </header>
  );
};

export default AppHeader;
