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
  sessionId?: string;               // optional session display
  statusText?: string;              // optional status, e.g. "Online"
}

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
  sessionId,
  statusText,
}) => {
  const [absolutePath, setAbsolutePath] = React.useState<string>('');
  const [inputValue, setInputValue] = React.useState<string>(workspace);
  const [showPopover, setShowPopover] = React.useState<boolean>(false);
  const [showClearConfirm, setShowClearConfirm] = React.useState<boolean>(false);
  const [showFilesPopover, setShowFilesPopover] = React.useState<boolean>(false);
  
  const popoverRef = React.useRef<HTMLDivElement>(null);
  const clearConfirmRef = React.useRef<HTMLDivElement>(null);
  const filesPopoverRef = React.useRef<HTMLDivElement>(null);

  // keep internal input synced when parent workspace prop changes
  React.useEffect(() => {
    setInputValue(workspace);
  }, [workspace]);

  // resolve workspace to absolute path via backend
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
    } catch {
      setAbsolutePath('Server unreachable');
    }
  }, []);

  // initial resolve and resolve on prop change
  React.useEffect(() => {
    resolveAbsolute(workspace);
  }, [workspace, resolveAbsolute]);

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
    };
    if (showPopover || showClearConfirm || showFilesPopover) {
      document.addEventListener('mousedown', handleOutsideClick);
    }
    return () => document.removeEventListener('mousedown', handleOutsideClick);
  }, [showPopover, showClearConfirm, showFilesPopover]);

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
