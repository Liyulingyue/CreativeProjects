import React from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';

interface ViewerTerminalStackProps {
  showFileViewer: boolean;
  selectedFilePath: string | null;
  fileContent: string | null;
  fileLoading: boolean;
  onClearSelection: () => void;
  termRef: React.RefObject<HTMLDivElement>;
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
  onClearSelection,
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
                    <button onClick={onClearSelection} className="close-btn">
                      ×
                    </button>
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
