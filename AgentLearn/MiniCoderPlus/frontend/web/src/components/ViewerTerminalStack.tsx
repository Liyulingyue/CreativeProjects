import React from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { Plus, X } from 'lucide-react';

export interface TerminalTab {
  id: string;
  title: string;
}

interface ViewerTerminalStackProps {
  showFileViewer: boolean;
  selectedFilePath: string | null;
  fileContent: string | null;
  fileLoading: boolean;
  isEditing?: boolean;
  onToggleEdit?: () => void;
  onCancel?: () => void;
  onClearSelection: () => void;
  onSave?: () => void;
  onContentChange?: (content: string) => void;
  termRef?: React.RefObject<HTMLDivElement | null>;
  // Multi-terminal support
  terminalTabs?: TerminalTab[];
  activeTerminalId?: string;
  onSelectTerminal?: (id: string) => void;
  onAddTerminal?: () => void;
  onCloseTerminal?: (id: string) => void;
  setTermRef?: (id: string, el: HTMLDivElement | null) => void;
  
  panelId?: string;
  order?: number;
  minSize?: number;
  viewerPanelId?: string;
  viewerHandleId?: string;
  terminalPanelId?: string;
  panelGroupId?: string;
  autoSaveId?: string;
  showTerminal?: boolean;
}

const ViewerTerminalStack: React.FC<ViewerTerminalStackProps> = ({
  showFileViewer,
  selectedFilePath,
  fileContent,
  fileLoading,
  isEditing = false,
  onToggleEdit,
  onCancel,
  onClearSelection,
  onSave,
  onContentChange,
  termRef,
  terminalTabs = [],
  activeTerminalId,
  onSelectTerminal,
  onAddTerminal,
  onCloseTerminal,
  setTermRef,
  panelId,
  order = 2,
  minSize = 30,
  viewerPanelId,
  viewerHandleId,
  terminalPanelId,
  panelGroupId,
  autoSaveId,
  showTerminal = true,
}) => (
  <Panel id={panelId} order={order} minSize={minSize}>
    <PanelGroup direction="vertical" id={panelGroupId} autoSaveId={autoSaveId}>
      {showFileViewer && (
        <>
          <Panel defaultSize={60} minSize={20} id={viewerPanelId} order={1}>
            <div className="file-viewer" style={{ height: '100%' }}>
              {selectedFilePath ? (
                <>
                  <div className="viewer-header">
                    <span>{selectedFilePath.split(/[/\\]/).pop()}</span>
                    <div className="viewer-actions">
                      {!isEditing ? (
                        <button onClick={onToggleEdit} className="edit-btn" title="Enter edit mode">
                          ✏️ 编辑
                        </button>
                      ) : (
                        <>
                          <button onClick={onSave} className="save-btn" title="Save changes">
                            💾 保存
                          </button>
                          <button onClick={onCancel || onToggleEdit} className="cancel-btn" title="Cancel editing">
                            ❌ 取消
                          </button>
                        </>
                      )}
                      <button onClick={onClearSelection} className="close-btn">
                        ×
                      </button>
                    </div>
                  </div>
                  {isEditing && onContentChange ? (
                    <textarea 
                      className="viewer-content editor-mode"
                      value={fileLoading ? 'Loading...' : (fileContent || '')}
                      onChange={(e) => onContentChange(e.target.value)}
                      disabled={fileLoading}
                      spellCheck={false}
                    />
                  ) : (
                    <pre className="viewer-content">
                      {fileLoading ? 'Loading...' : fileContent}
                    </pre>
                  )}
                </>
              ) : (
                <div className="viewer-placeholder">
                  <div className="placeholder-content">
                    <span className="icon">📄</span>
                    <p>选择一个文件进行预览</p>
                  </div>
                </div>
              )}
            </div>
          </Panel>
          {showTerminal && <PanelResizeHandle className="v-resizer" id={viewerHandleId} />}
        </>
      )}

      {showTerminal && (
        <Panel id={terminalPanelId} order={2} minSize={15} defaultSize={showFileViewer ? 40 : 100}>
          <div className="terminal-wrapper" style={{ height: '100%', background: '#1e1e1e', display: 'flex', flexDirection: 'column' }}>
            {/* Terminal Tab Bar */}
            {terminalTabs.length > 0 && (
              <div className="terminal-tabs-container">
                {terminalTabs.map(tab => (
                  <div 
                    key={tab.id}
                    className={`terminal-tab ${activeTerminalId === tab.id ? 'active' : ''}`}
                    onClick={() => onSelectTerminal?.(tab.id)}
                  >
                    <span>{tab.title}</span>
                    <X 
                      size={14} 
                      className="close-icon"
                      onClick={(e) => {
                        e.stopPropagation();
                        onCloseTerminal?.(tab.id);
                      }}
                    />
                  </div>
                ))}
                <button 
                  onClick={onAddTerminal}
                  className="add-terminal-btn"
                  title="Open new terminal"
                >
                  <Plus size={16} />
                </button>
              </div>
            )}

            <div style={{ flex: 1, position: 'relative' }}>
              {/* Backward compatibility for single terminal */}
              {terminalTabs.length === 0 && termRef && (
                <div ref={termRef} className="xterm-container" style={{ height: '100%' }} />
              )}
              
              {/* Multi-terminal containers */}
              {terminalTabs.map(tab => (
                <div 
                  key={tab.id}
                  ref={el => setTermRef?.(tab.id, el)}
                  className="xterm-container"
                  style={{ 
                    height: '100%', 
                    display: activeTerminalId === tab.id ? 'block' : 'none' 
                  }}
                />
              ))}
            </div>
          </div>
        </Panel>
      )}
    </PanelGroup>
  </Panel>
);

export default ViewerTerminalStack;
