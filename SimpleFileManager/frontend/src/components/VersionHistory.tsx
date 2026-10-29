import { useCallback, useEffect, useState } from 'react';
import {
  fetchVersions, restoreVersion, deleteVersion, type FileVersion,
} from '../api';
import { Icon } from './ui/Icon';

interface VersionHistoryProps {
  path: string;
  name: string;
  onClose: () => void;
  onRestored: () => void;
}

/**
 * What a file used to contain.
 *
 * A version is kept whenever the service replaces a file, so an
 * upload that was not what was meant can be taken back. Restoring
 * keeps what was there too, so a restore can itself be undone.
 */
export function VersionHistory({ path, name, onClose, onRestored }: VersionHistoryProps) {
  const [versions, setVersions] = useState<FileVersion[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await fetchVersions(path);
      setVersions(data.versions);
    } catch (e) {
      setError(e instanceof Error ? e.message : '读取失败');
    } finally {
      setLoading(false);
    }
  }, [path]);

  useEffect(() => { load(); }, [load]);

  const restore = async (version: FileVersion) => {
    setBusyId(version.id); setError(null);
    try {
      await restoreVersion(version.id);
      await load();
      onRestored();
    } catch (e) {
      setError(e instanceof Error ? e.message : '还原失败');
    } finally {
      setBusyId(null); }
  };

  const remove = async (version: FileVersion) => {
    if (!window.confirm('删除这个历史版本？此操作不可恢复。')) return;
    setBusyId(version.id); setError(null);
    try {
      await deleteVersion(version.id);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : '删除失败');
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" onClick={onClose}>
      <div className="relative bg-white rounded-xl shadow-popover w-full max-w-lg max-h-[80vh] flex flex-col overflow-hidden" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-100 flex-shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            <Icon name="clock" size={18} className="text-indigo-500 flex-shrink-0" />
            <span className="text-sm font-semibold text-slate-800 truncate">历史版本 · {name}</span>
          </div>
          <button onClick={onClose} className="w-9 h-9 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors">
            <Icon name="x" size={18} />
          </button>
        </div>

        <div className="p-5 overflow-y-auto">
          {error && <div className="text-sm text-red-500 mb-3">{error}</div>}
          {loading ? (
            <div className="flex items-center justify-center py-8 text-slate-400">
              <div className="loading-spinner mr-3" /><span>加载中...</span>
            </div>
          ) : versions.length === 0 ? (
            <div className="text-center py-10 text-slate-400">
              <Icon name="clock" size={36} className="text-slate-300 mb-3" />
              <div className="text-sm">这个文件还没有历史版本</div>
              <div className="text-xs mt-1">通过服务覆盖文件时，旧的内容会留在这里</div>
            </div>
          ) : (
            <div className="space-y-2">
              {versions.map(v => (
                <div key={v.id} className="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-slate-200">
                  <Icon name="fileText" size={18} className="text-slate-400 flex-shrink-0" />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-slate-700 truncate">{v.human}</div>
                    <div className="text-[11px] text-slate-400">
                      {(v.size / 1024).toFixed(1)} KB{v.note && ` · ${v.note}`}
                    </div>
                  </div>
                  <div className="flex items-center gap-1 flex-shrink-0">
                    <button onClick={() => restore(v)} disabled={busyId === v.id}
                      className="px-2.5 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
                      {busyId === v.id ? '处理中...' : '还原'}
                    </button>
                    <button onClick={() => remove(v)} disabled={busyId === v.id}
                      className="px-2.5 py-1.5 rounded-lg text-red-500 text-xs hover:bg-red-50 disabled:opacity-40 transition-colors">
                      删除
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
