import { createContext, useCallback, useContext, useState, type ReactNode } from 'react';
import { Icon } from './Icon';

type ToastType = 'success' | 'error' | 'info';

interface Toast {
  id: string;
  type: ToastType;
  message: string;
}

interface ToastContextValue {
  toast: (message: string, type?: ToastType) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used within ToastProvider');
  return ctx;
}

const ICON_MAP: Record<ToastType, { icon: string; bg: string; iconBg: string }> = {
  success: { icon: 'check', bg: 'bg-white border-slate-200', iconBg: 'bg-green-100 text-green-600' },
  error: { icon: 'x', bg: 'bg-white border-slate-200', iconBg: 'bg-red-100 text-red-600' },
  info: { icon: 'info', bg: 'bg-white border-slate-200', iconBg: 'bg-indigo-100 text-indigo-600' },
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const toast = useCallback((message: string, type: ToastType = 'info') => {
    const id = Math.random().toString(36).slice(2);
    setToasts(prev => [...prev, { id, type, message }]);
    setTimeout(() => setToasts(prev => prev.filter(t => t.id !== id)), 4000);
  }, []);

  const dismiss = (id: string) => setToasts(prev => prev.filter(t => t.id !== id));

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 pointer-events-none">
        {toasts.map(t => {
          const meta = ICON_MAP[t.type];
          return (
            <div
              key={t.id}
              className={`pointer-events-auto flex items-center gap-3 pl-3 pr-2 py-2.5 rounded-xl shadow-popover border ${meta.bg} animate-toast max-w-md`}
            >
              <span className={`flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center ${meta.iconBg}`}>
                <Icon name={meta.icon} size={14} />
              </span>
              <span className="flex-1 text-sm text-slate-700">{t.message}</span>
              <button onClick={() => dismiss(t.id)} className="flex-shrink-0 p-1 rounded text-slate-400 hover:text-slate-600 hover:bg-slate-100">
                <Icon name="x" size={14} />
              </button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}
