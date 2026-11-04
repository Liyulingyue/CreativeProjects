import React, { useEffect, useState } from 'react';
import { Icon } from './Icon';

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}

export function Dialog({ open, title, onClose, children }: DialogProps) {
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape' && open) onClose(); };
    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/30 backdrop-blur-sm" onClick={onClose} />
      <div className="relative bg-white rounded-xl shadow-popover max-w-md w-full mx-3 sm:mx-4" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-4 border-b border-slate-100">
          <h3 className="text-base font-semibold text-slate-900">{title}</h3>
          <button onClick={onClose} className="w-8 h-8 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors">
            <Icon name="x" size={18} />
          </button>
        </div>
        <div className="px-5 py-4">{children}</div>
      </div>
    </div>
  );
}

interface SheetProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  size?: 'default' | 'lg';
}

/**
 * Bottom sheet on a phone, centred dialog from sm up.
 *
 * A centred dialog is the right shape for "answer this question", but
 * not for "let me look at some detail and get back to what I was
 * doing" — on a 390px screen the panel floats in the middle with
 * nothing beneath it, and dismissing it means finding the close
 * button. A sheet rises from the bottom edge the way the platform
 * does, keeps the page visible above it, and dismisses by tapping
 * away.
 */
export function Sheet({ open, title, onClose, children, size = 'default' }: SheetProps) {
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => { if (e.key === 'Escape' && open) onClose(); };
    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-end sm:items-center sm:justify-center">
      <div className="fixed inset-0 bg-black/30 backdrop-blur-sm" onClick={onClose} />
      <div
        className={`relative bg-white rounded-t-2xl sm:rounded-xl shadow-popover w-full ${size === 'lg' ? 'sm:max-w-2xl' : 'sm:max-w-md'} sm:mx-4 max-h-[85vh] flex flex-col`}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-slate-100 flex-shrink-0">
          <h3 className="text-base font-semibold text-slate-900">{title}</h3>
          <button onClick={onClose} aria-label="关闭" className="w-8 h-8 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors">
            <Icon name="x" size={18} />
          </button>
        </div>
        <div className="px-5 py-4 overflow-y-auto">{children}</div>
      </div>
    </div>
  );
}

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: string;
  confirmText?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ open, title, message, confirmText = '确认', danger, onConfirm, onCancel }: ConfirmDialogProps) {
  return (
    <Dialog open={open} title={title} onClose={onCancel}>
      <p className="text-sm text-slate-600 mb-5 leading-relaxed">{message}</p>
      <div className="flex justify-end gap-2">
        <button onClick={onCancel} className="px-4 py-2 rounded-lg text-sm text-slate-600 hover:bg-slate-100 transition-colors font-medium">
          取消
        </button>
        <button
          onClick={onConfirm}
          className={`px-4 py-2 rounded-lg text-white text-sm font-medium transition-colors ${danger ? 'bg-red-600 hover:bg-red-700' : 'bg-indigo-600 hover:bg-indigo-700'}`}
        >
          {confirmText}
        </button>
      </div>
    </Dialog>
  );
}

interface UploadConflictDialogProps {
  open: boolean;
  filename: string;
  /** What is already at the destination. */
  existing?: { name: string; size: number; modified?: string };
  onReplace: () => void;
  onRename: () => void;
  onCancel: () => void;
}

/**
 * What to do about a name that is already taken.
 *
 * Replacing it is offered first because it is what the user usually
 * means, but it is the one that destroys something, so it is styled
 * as the dangerous one and the renamed copy is the safe default
 * middle ground.
 */
export function UploadConflictDialog({ open, filename, existing, onReplace, onRename, onCancel }: UploadConflictDialogProps) {
  const sizeText = existing?.size
    ? (existing.size >= 1024 * 1024
      ? `${(existing.size / 1024 / 1024).toFixed(1)} MB`
      : `${Math.max(1, Math.round(existing.size / 1024))} KB`)
    : '';
  return (
    <Dialog open={open} title="同名文件已存在" onClose={onCancel}>
      <p className="text-sm text-slate-600 mb-1 leading-relaxed">
        目标位置已经有 <strong className="text-slate-800">{filename}</strong>
        {sizeText && <> （{sizeText}）</>}
        {existing?.modified && <> · {existing.modified}</>}。
      </p>
      <p className="text-xs text-slate-400 mb-5">
        覆盖会把原来的文件换掉，且无法从回收站找回来。
      </p>
      <div className="flex flex-wrap justify-end gap-2">
        <button onClick={onCancel} className="px-4 py-2 rounded-lg text-sm text-slate-600 hover:bg-slate-100 transition-colors font-medium">
          取消上传
        </button>
        <button onClick={onRename} className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
          换个名字上传
        </button>
        <button onClick={onReplace} className="px-4 py-2 rounded-lg bg-red-600 text-white text-sm font-medium hover:bg-red-700 transition-colors">
          覆盖原文件
        </button>
      </div>
    </Dialog>
  );
}

interface PromptDialogProps {
  open: boolean;
  title: string;
  message: string;
  defaultValue?: string;
  onConfirm: (value: string) => void;
  onCancel: () => void;
}

export function PromptDialog({ open, title, message, defaultValue = '', onConfirm, onCancel }: PromptDialogProps) {
  const [value, setValue] = useState(defaultValue);
  useEffect(() => { if (open) setValue(defaultValue); }, [open, defaultValue]);

  return (
    <Dialog open={open} title={title} onClose={onCancel}>
      <p className="text-sm text-slate-600 mb-3">{message}</p>
      <input
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Enter' && value.trim()) onConfirm(value); }}
        autoFocus
        className="w-full px-3 py-2.5 rounded-lg border border-slate-200 text-base sm:text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
      />
      <div className="flex justify-end gap-2 mt-5">
        <button onClick={onCancel} className="px-4 py-2 rounded-lg text-sm text-slate-600 hover:bg-slate-100 transition-colors font-medium">取消</button>
        <button
          onClick={() => value.trim() && onConfirm(value)}
          disabled={!value.trim()}
          className="px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-700 text-white text-sm font-medium transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          确认
        </button>
      </div>
    </Dialog>
  );
}
