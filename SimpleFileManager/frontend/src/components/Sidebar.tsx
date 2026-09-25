import { useState } from 'react';
import type { TreeNode } from '../api';
import { Icon } from './ui/Icon';

interface SidebarProps {
  tree: TreeNode | null;
  onNavigate: (path: string) => void;
  currentPath: string;
}

function TreeItem({ node, level, currentPath, onNavigate }: TreeItemProps) {
  const [expanded, setExpanded] = useState(level < 2);
  const isActive = currentPath === node.path;
  const hasChildren = node.children && node.children.length > 0;

  const handleClick = () => {
    if (node.is_dir) {
      if (hasChildren) setExpanded(!expanded);
      onNavigate(node.path);
    } else {
      onNavigate(node.path);
    }
  };

  return (
    <div>
      <div
        className={`flex items-center gap-1 py-1.5 pr-2 rounded-md cursor-pointer text-sm transition-colors ${
          isActive ? 'bg-indigo-50 text-indigo-700 font-medium' : 'text-slate-600 hover:bg-slate-100'
        }`}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleClick}
      >
        {hasChildren ? (
          <button
            onClick={(e) => { e.stopPropagation(); setExpanded(!expanded); }}
            className="w-4 flex items-center justify-center text-slate-400 flex-shrink-0 hover:text-slate-600"
          >
            <Icon name={expanded ? 'chevronDown' : 'chevronRight'} size={14} />
          </button>
        ) : (
          <span className="w-4 flex-shrink-0" />
        )}
        <Icon name="folder" size={16} className={isActive ? 'text-indigo-500 flex-shrink-0' : 'text-amber-500 flex-shrink-0'} />
        <span className="flex-1 min-w-0 truncate">{node.name}</span>
      </div>
      {hasChildren && expanded && (
        <div>
          {node.children!.map((child) => (
            <TreeItem key={child.path} node={child} level={level + 1} currentPath={currentPath} onNavigate={onNavigate} />
          ))}
        </div>
      )}
    </div>
  );
}

interface TreeItemProps {
  node: TreeNode;
  level: number;
  currentPath: string;
  onNavigate: (path: string) => void;
}

export function Sidebar({ tree, onNavigate, currentPath }: SidebarProps) {
  return (
    <div className="w-56 bg-white border-r border-slate-200 flex flex-col overflow-hidden flex-shrink-0">
      <div className="px-4 py-2.5 text-xs font-semibold text-slate-400 uppercase tracking-wider border-b border-slate-100 flex items-center gap-2">
        <Icon name="folder" size={14} />
        文件夹
      </div>
      <div className="flex-1 overflow-y-auto px-2 py-1.5">
        {!tree ? (
          <div className="flex items-center justify-center py-8 text-slate-400 text-sm">加载中...</div>
        ) : (
          <TreeItem node={tree} level={0} currentPath={currentPath} onNavigate={onNavigate} />
        )}
      </div>
    </div>
  );
}
