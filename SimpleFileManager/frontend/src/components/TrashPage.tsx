import { useCallback, useEffect, useState } from 'react';
import {
  emptyTrash, fetchCurrentUser, fetchTrash, fetchTrashLog,
  purgeExpiredTrash, purgeTrashEntry, restoreTrashEntry,
} from '../api';
import type { TrashEntry, TrashLogEntry } from '../api';
import { Icon } from './ui/Icon';

function formatSize(bytes: number): string {
  if (!bytes) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

/**
 * The recycle bin.
 *
 * Deleting a file used to be permanent, which on a NAS is a way to
 * lose something with no way back. Deleted files now wait here, with
 * the path they came from, until they are restored or the bin is
 * emptied.
 */
export function TrashPage() {
  const [entries, setEntries] = useState<TrashEntry[]>([]);
  const [totalSize, setTotalSize] = useState(0);
  const [retentionDays, setRetentionDays] = useState(0);
  const [cleanupLog, setCleanupLog] = useState<TrashLogEntry[]>([]);
  const [isAdmin, setIsAdmin] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [busyId, setBusyId] = useState<string>('');
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await fetchTrash();
      setEntries(data.entries);
      setTotalSize(data.size);
      setRetentionDays(data.retention_days ?? 0);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setIsLoading(false);
    }
  }, []);

  const loadLog = useCallback(async () => {
    try {
      const data = await fetchTrashLog();
      setCleanupLog(data.entries);
    } catch {
      // The log is administrator-only; a family member simply does not
      // see it.
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    fetchCurrentUser()
      .then(u => { setIsAdmin(u.role === 'admin'); if (u.role === 'admin') loadLog(); })
      .catch(() => {});
  }, [loadLog]);

  const restore = async (entry: TrashEntry) => {
    setBusyId(entry.id); setMessage(null);
    try {
      const res = await restoreTrashEntry(entry.id);
      setMessage(res.message || `已还原 ${entry.name}`);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '还原失败');
    } finally { setBusyId(''); }
  };

  const purge = async (entry: TrashEntry) => {
    if (!window.confirm(`永久删除 "${entry.name}"？此操作不可恢复。`)) return;
    setBusyId(entry.id); setMessage(null);
    try {
      await purgeTrashEntry(entry.id);
      setMessage(`已永久删除 ${entry.name}`);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '删除失败');
    } finally { setBusyId(''); }
  };

  const purgeExpired = async () => {
    setBusyId('__expired__'); setMessage(null);
    try {
      const res = await purgeExpiredTrash();
      setMessage(res.message || `已清理 ${res.purged} 个过期项目`);
      await load();
      await loadLog();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '清理失败');
    } finally { setBusyId(''); }
  };

  const empty = async () => {
    if (!window.confirm(`清空回收站？${entries.length} 项将被永久删除，无法恢复。`)) return;
    setBusyId('__all__'); setMessage(null);
    try {
      const res = await emptyTrash();
      setMessage(res.message || '已清空回收站');
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '清空失败');
    } finally { setBusyId(''); }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center justify-between flex-shrink-0 gap-3">
        <div className="flex items-center gap-3 min-w-0">
          <Icon name="trash" size={20} className="text-slate-500 flex-shrink-0" />
          <span className="text-base font-semibold text-slate-800 flex-shrink-0">回收站</span>
          <span className="hidden sm:block text-xs text-slate-400 truncate">
            {entries.length > 0
              ? `${entries.length} 项 · ${formatSize(totalSize)}`
              : '删除的文件会先放到这里，可以随时还原'}
          </span>
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          {isAdmin && retentionDays > 0 && (
            <button onClick={purgeExpired} disabled={busyId === '__expired__'}
              title="删除所有超过保留期的项目"
              className="flex items-center gap-1.5 px-3 py-2 rounded-lg border border-slate-200 text-slate-600 text-sm font-medium hover:bg-slate-50 disabled:opacity-50 transition-colors">
              <Icon name="clock" size={16} />{busyId === '__expired__' ? '清理中...' : '清理过期项'}
            </button>
          )}
          {isAdmin && entries.length > 0 && (
            <button onClick={empty} disabled={busyId === '__all__'}
              className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-red-600 text-white text-sm font-medium hover:bg-red-700 disabled:opacity-50 transition-colors flex-shrink-0">
              <Icon name="trash" size={16} />{busyId === '__all__' ? '清空中...' : '清空回收站'}
            </button>
          )}
        </div>
      </div>

      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-2 text-xs text-slate-500 flex items-center gap-2 flex-wrap">
        <Icon name="clock" size={13} className="text-slate-400" />
        {retentionDays > 0 ? (
          <span>删除的文件会保留 <strong className="text-slate-700">{retentionDays}</strong> 天，之后自动清理。</span>
        ) : (
          <span>删除的文件会一直保留，直到你手动清空。</span>
        )}
        {isAdmin && (
          <span className="text-slate-400">可在「设置」里调整保留天数。</span>
        )}
      </div>

      {message && <div className="px-4 sm:px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>
        ) : entries.length === 0 ? (
          <div className="text-center py-16 text-slate-400">
            <Icon name="trash" size={40} className="mx-auto text-slate-300 mb-3" />
            <div className="text-base">回收站是空的</div>
            <div className="text-sm mt-1">删除的文件会先放到这里，而不是直接消失</div>
          </div>
        ) : (
          <div className="bg-white rounded-lg border border-slate-200 overflow-hidden">
            {entries.map(entry => (
              <div key={entry.id}
                className="flex items-center gap-3 px-3 sm:px-4 py-3 border-b border-slate-100 last:border-b-0">
                <Icon name={entry.is_dir ? 'folder' : 'fileText'} size={18}
                  className="text-slate-400 flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-slate-800 truncate">{entry.name}</div>
                  <div className="text-[11px] text-slate-400 truncate">
                    {entry.rel_path || '（原路径未知）'} · {entry.deleted_at_human}
                    {entry.size > 0 && ` · ${formatSize(entry.size)}`}
                  </div>
                  {entry.expires_in_days !== null && entry.expires_in_days !== undefined && (
                    <div className={`text-[11px] mt-0.5 ${entry.expires_in_days <= 1 ? 'text-amber-600' : 'text-slate-400'}`}>
                      {entry.expires_in_days <= 1 ? '今天将被自动清理' : `还有 ${entry.expires_in_days} 天自动清理`}
                    </div>
                  )}
                </div>
                <div className="flex items-center gap-1 flex-shrink-0">
                  <button onClick={() => restore(entry)} disabled={busyId === entry.id}
                    className="px-2 py-1 rounded text-xs text-indigo-600 hover:bg-indigo-50 disabled:opacity-40 transition-colors">
                    {busyId === entry.id ? '处理中...' : '还原'}
                  </button>
                  <button onClick={() => purge(entry)} disabled={busyId === entry.id}
                    className="px-2 py-1 rounded text-xs text-red-500 hover:bg-red-50 disabled:opacity-40 transition-colors">
                    永久删除
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {isAdmin && cleanupLog.length > 0 && (
          <div className="mt-6 bg-white rounded-lg border border-slate-200 overflow-hidden">
            <div className="px-4 py-2 border-b border-slate-100 text-xs font-medium text-slate-500">
              自动清理记录
            </div>
            {cleanupLog.map(rec => (
              <div key={rec.id}
                className="flex items-center gap-3 px-4 py-2 border-b border-slate-50 last:border-b-0 text-xs text-slate-500">
                <Icon name="trash" size={13} className="text-slate-300 flex-shrink-0" />
                <span className="flex-1 min-w-0 truncate text-slate-700">{rec.name}</span>
                <span className="text-slate-400 flex-shrink-0">
                  {rec.size > 0 && `${formatSize(rec.size)} · `}
                  {new Date(rec.ts * 1000).toLocaleString()}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
