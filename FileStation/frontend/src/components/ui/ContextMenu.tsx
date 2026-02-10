import { useEffect, useRef } from 'react';

export interface ContextMenuItem {
  label: string;
  icon: string;
  onClick: () => void;
  danger?: boolean;
}

export interface ContextMenuProps {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}

export default function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  // Adjust position if menu goes off screen
  const menuWidth = 160;
  const menuHeight = items.length * 36 + 16;
  const adjustedX = x + menuWidth > window.innerWidth ? x - menuWidth : x;
  const adjustedY = y + menuHeight > window.innerHeight ? y - menuHeight : y;

  return (
    <div
      ref={menuRef}
      className="fixed z-[100] w-40 bg-white/90 backdrop-blur-xl border border-slate-200/60 shadow-2xl rounded-xl py-2 overflow-hidden animate-in fade-in zoom-in duration-100"
      style={{ left: adjustedX, top: adjustedY }}
    >
      {items.map((item, index) => (
        <button
          key={index}
          onClick={(e) => {
            e.stopPropagation();
            item.onClick();
            onClose();
          }}
          className={`w-full px-4 py-2 text-left flex items-center space-x-3 transition-colors ${
            item.danger 
              ? 'text-red-600 hover:bg-red-50' 
              : 'text-slate-700 hover:bg-indigo-50 hover:text-indigo-600'
          }`}
        >
          <span className="text-base w-6 text-center flex-shrink-0">{item.icon}</span>
          <span className="text-[11px] font-bold uppercase tracking-wider">{item.label}</span>
        </button>
      ))}
    </div>
  );
}
