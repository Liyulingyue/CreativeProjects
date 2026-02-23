import React from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';

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
  termRef: React.RefObject<HTMLDivElement | null>;
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
                          ✏️ Edit
                        </button>
                      ) : (
                        <>
                          <button onClick={onSave} className="save-btn" title="Save changes">
                            💾 Save
                          </button>
                          <button onClick={onCancel || onToggleEdit} className="cancel-btn" title="Cancel editing">
                            ❌ Cancel
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
                    <p>Select a file to preview</p>
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
          <div className="terminal-wrapper" style={{ height: '100%', background: '#1e1e1e' }}>
            <div ref={termRef} className="xterm-container" style={{ height: '100%' }} />
          </div>
        </Panel>
      )}
    </PanelGroup>
  </Panel>
);

export default ViewerTerminalStack;
