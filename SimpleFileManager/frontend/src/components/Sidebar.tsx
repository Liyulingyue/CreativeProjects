import { useState } from 'react';
import { type TreeNode } from '../api';

interface SidebarProps {
  tree: TreeNode | null;
  onNavigate: (path: string) => void;
  currentPath: string;
}

interface TreeItemProps {
  node: TreeNode;
  level: number;
  currentPath: string;
  onNavigate: (path: string) => void;
}

function TreeItem({ node, level, currentPath, onNavigate }: TreeItemProps) {
  const [expanded, setExpanded] = useState(true);
  const isActive = currentPath === node.path;
  const hasChildren = node.children && node.children.length > 0;

  const handleClick = () => {
    if (node.is_dir) {
      if (hasChildren) {
        setExpanded(!expanded);
      }
      onNavigate(node.path);
    } else {
      onNavigate(node.path);
    }
  };

  return (
    <div>
      <div
        className={`tree-item ${isActive ? 'active' : ''}`}
        style={{ paddingLeft: `${level * 1 + 0.5}rem` }}
        onClick={handleClick}
      >
        {hasChildren ? (
          <span style={{ width: 16, textAlign: 'center' }}>
            {expanded ? '▼' : '▶'}
          </span>
        ) : (
          <span style={{ width: 16 }} />
        )}
        <span>{node.is_dir ? '📁' : '📄'}</span>
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {node.name}
        </span>
      </div>
      {hasChildren && expanded && (
        <div className="tree-children">
          {node.children!.map((child) => (
            <TreeItem
              key={child.path}
              node={child}
              level={level + 1}
              currentPath={currentPath}
              onNavigate={onNavigate}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function Sidebar({ tree, onNavigate, currentPath }: SidebarProps) {
  if (!tree) {
    return (
      <div className="sidebar">
        <div className="sidebar-header">
          <span>Folders</span>
        </div>
        <div className="sidebar-content">
          <div className="loading">Loading...</div>
        </div>
      </div>
    );
  }

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <span>Folders</span>
      </div>
      <div className="sidebar-content">
        <TreeItem node={tree} level={0} currentPath={currentPath} onNavigate={onNavigate} />
      </div>
    </div>
  );
}
