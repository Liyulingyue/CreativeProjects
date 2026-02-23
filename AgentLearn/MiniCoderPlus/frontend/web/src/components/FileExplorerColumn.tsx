import React, { useMemo, useState } from 'react';
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

type TreeNode = {
  name: string;
  abs_path: string;
  relativePath: string;  // path relative to workspace
  type: 'file' | 'directory';
  children?: TreeNode[];
};

const buildTree = (items: FileItem[]): TreeNode[] => {
  // Build tree using relative paths within workspace
  const nodesByPath: Record<string, TreeNode> = {};
  
  for (const it of items) {
    // Use 'path' (relative) instead of abs_path
    const clean = it.path.replace(/\\/g, '/');
    const parts = clean.split('/').filter(Boolean);
    let pathAcc = '';
    
    for (let i = 0; i < parts.length; i++) {
      pathAcc = pathAcc ? `${pathAcc}/${parts[i]}` : parts[i];
      
      // If this is the final part and we're on the input item
      const isLastPart = i === parts.length - 1;
      
      if (!nodesByPath[pathAcc]) {
        nodesByPath[pathAcc] = { 
          name: parts[i], 
          abs_path: isLastPart ? it.abs_path : it.abs_path,  // use current item's abs_path when final
          relativePath: pathAcc, 
          type: isLastPart ? it.type : 'directory',  // infer intermediate as directory
          children: [] 
        };
      } else {
        // If already exists and this is the final part, update to match the actual item type and abs_path
        if (isLastPart) {
          nodesByPath[pathAcc].type = it.type;
          nodesByPath[pathAcc].abs_path = it.abs_path;
        }
      }
      
      if (i > 0) {
        const parent = parts.slice(0, i).join('/');
        nodesByPath[parent].children = nodesByPath[parent].children || [];
        // avoid duplicates
        if (!nodesByPath[parent].children!.some(c => c.relativePath === nodesByPath[pathAcc].relativePath)) {
          nodesByPath[parent].children!.push(nodesByPath[pathAcc]);
        }
      }
    }
  }

  // find top-level nodes (no '/' in relativePath)
  const topLevel = Object.values(nodesByPath).filter(n => n.relativePath.indexOf('/') === -1);
  
  // sort children
  const sortTree = (arr: TreeNode[] | undefined) => {
    if (!arr) return;
    arr.sort((a, b) => {
      if (a.type === b.type) return a.name.localeCompare(b.name);
      return a.type === 'directory' ? -1 : 1;
    });
    arr.forEach(n => sortTree(n.children));
  };
  sortTree(topLevel);

  return topLevel;
};

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
}) => {
  const tree = useMemo(() => buildTree(explorerData || []), [explorerData]);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [loading, setLoading] = useState<Record<string, boolean>>({});
  const [dynamicChildren, setDynamicChildren] = useState<Record<string, TreeNode[]>>({});

  const toggleExpand = async (path: string, absPath: string) => {
    const isExpanded = expanded[path];
    
    if (!isExpanded && !dynamicChildren[path]) {
      // Loading children if not already loaded
      setLoading(prev => ({ ...prev, [path]: true }));
      try {
        const resp = await fetch(`/api/v1/files/list?path=${encodeURIComponent(absPath)}`);
        if (resp.ok) {
          const data = await resp.json();
          const newChildren = (data.files || []).map((item: any) => ({
            name: item.name,
            abs_path: item.abs_path,
            relativePath: `${path}/${item.name}`,
            type: item.type,
            children: []
          }));
          setDynamicChildren(prev => ({ ...prev, [path]: newChildren }));
        }
      } catch (e) {
        console.error('Failed to load directory', e);
      } finally {
        setLoading(prev => ({ ...prev, [path]: false }));
      }
    }
    
    setExpanded(prev => ({ ...prev, [path]: !prev[path] }));
  };

  const renderNode = (node: TreeNode, depth = 0) => {
    const isDir = node.type === 'directory';
    const isExpanded = expanded[node.relativePath];
    const isLoadingChildren = loading[node.relativePath];
    const childrenToRender = dynamicChildren[node.relativePath] || node.children || [];
    
    return (
      <div key={node.relativePath}>
        <div
          className={`explorer-item ${node.type} ${selectedFilePath === node.abs_path ? 'selected' : ''} ${!showFileViewer && node.type === 'file' ? 'no-peek' : ''}`}
          style={{ paddingLeft: 12 + depth * 12 }}
          onClick={() => {
            if (isDir) toggleExpand(node.relativePath, node.abs_path);
            else if (node.type === 'file' && showFileViewer) onSelect(node.abs_path);
          }}
        >
          {isDir ? (isExpanded ? '📂' : '📁') : '📄'} {node.name}
          {isLoadingChildren && <span style={{ marginLeft: '4px', fontSize: '11px' }}>⏳</span>}
        </div>
        {isDir && isExpanded && childrenToRender.length > 0 && childrenToRender.map((c) => renderNode(c, depth + 1))}
        {isDir && isExpanded && childrenToRender.length === 0 && !isLoadingChildren && (
          <div style={{ paddingLeft: 12 + (depth + 1) * 12, fontSize: '12px', color: '#9ca3af' }}>
            (empty)
          </div>
        )}
      </div>
    );
  };

  return (
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
            {tree.map(n => renderNode(n))}
          </div>
        </div>
      </Panel>
      <PanelResizeHandle className="h-resizer" id={handleId} />
    </>
  );
};

export default FileExplorerColumn;
