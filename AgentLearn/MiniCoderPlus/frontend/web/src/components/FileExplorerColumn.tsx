import React from 'react';
import { Panel, PanelResizeHandle } from 'react-resizable-panels';
import type { FileItem } from '../types';

interface FileExplorerColumnProps {
  explorerData: FileItem[];
  selectedFilePath: string | null;
  showFileViewer: boolean;
  onRefresh: () => void;
  onSelect: (absPath: string) => void;
  title?: string;
  panelId?: string;
  handleId?: string;
  order?: number;
  defaultSize?: number;
  minSize?: number;
}

const FileExplorerColumn: React.FC<FileExplorerColumnProps> = ({
  explorerData,
  selectedFilePath,
  showFileViewer,
  onRefresh,
  onSelect,
  title = 'FILES',
  panelId,
  handleId,
  order = 1,
  defaultSize = 20,
  minSize = 10,
}) => (
  <>
    <Panel id={panelId} order={order} defaultSize={defaultSize} minSize={minSize}>
      <div className="file-explorer">
        <div className="explorer-header">
          <span>{title}</span>
          <button onClick={onRefresh} className="refresh-btn">
            🔄
          </button>
        </div>
        <div className="explorer-list">
          {explorerData.map((item, index) => (
            <div
              key={index}
              className={`explorer-item ${item.type} ${selectedFilePath === item.abs_path ? 'selected' : ''} ${
                !showFileViewer && item.type === 'file' ? 'no-peek' : ''
              }`}
              onClick={() => {
                if (item.type === 'file' && showFileViewer) onSelect(item.abs_path);
              }}
            >
              {item.type === 'directory' ? '📁' : '📄'} {item.name}
            </div>
          ))}
        </div>
      </div>
    </Panel>
    <PanelResizeHandle className="h-resizer" id={handleId} />
  </>
);

export default FileExplorerColumn;
