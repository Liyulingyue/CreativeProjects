import { useCallback, useEffect, useState } from 'react';
import { Icon } from './ui/Icon';

interface UploadEntry {
  id: number;
  at: number;
  username: string;
  path: string;
  name: string;
  size: number;
}

interface UploadStats {
  count: number;
  oldest: number | null;
  newest: number | null;
  total_bytes: number;
  retention_days: number;
  max_rows: number;
}

interface UploadLogState {
  entries: UploadEntry[];
  count: number;
  stats: UploadStats;
}

function formatSize(bytes: number): string {
  if (!bytes) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

function formatTime(at: number): string {
  const d = new Date(at * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} `
    + `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function authHeaders(): Record<string, string> {
  const token = localStorage.getItem('sfm_token');
  return token ? { Authorization: `Bearer ${token}` } : {};
}

const PAGE_SIZE = 50;

/**
 * A history of uploads made through this service.
 *
 * It says who put what where, and when -- nothing more. A file that
 * arrived by SMB, rsync or a shell is not in here, so the absence of an
 * entry does not mean "nobody uploaded it". The log is capped by age
 * and by row count rather than kept forever, because an unbounded
 * history is a disk-filling problem waiting to happen.
 */
export function UploadLogPanel() {
  const [state, setState] = useState<UploadLogState | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async (limit = PAGE_SIZE) => {
    try {
      const res = await fetch(`/api/uploads?limit=${limit}`, { headers: authHeaders() });
      if (!res.ok) throw new Error('加载上传记录失败');
      setState(await res.json());
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const clearAll = async () => {
    if (!state?.count) return;
    if (!window.confirm('清空全部上传记录？这只删除记录，不影响任何文件。')) return;
    setBusy(true);
    setMessage(null);
    try {
      const res = await fetch('/api/uploads', { method: 'DELETE', headers: authHeaders() });
      if (!res.ok) {
        const b = await res.json().catch(() => ({}));
        throw new Error(typeof b?.detail === 'string' ? b.detail : '清空失败');
      }
      await load();
      setMessage('上传记录已清空');
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '清空失败');
    } finally {
      setBusy(false);
    }
  };

  const stats = state?.stats;
  const shown = state?.entries.length ?? 0;
  const total = state?.count ?? 0;

  return (
    <div className="space-y-3">
      {message && (
        <div className="text-sm text-indigo-700 bg-indigo-50 rounded-lg px-3 py-2">{message}</div>
      )}

      {stats && (
        <div className="flex items-center gap-2 flex-wrap text-[11px] text-slate-400">
          <span>共 {stats.count} 条</span>
          {stats.newest && <span>· 最近 {formatTime(stats.newest)}</span>}
          {stats.total_bytes > 0 && <span>· 合计 {formatSize(stats.total_bytes)}</span>}
          <span>· 保留 {stats.retention_days} 天，最多 {stats.max_rows} 条</span>
          <div className="flex-1" />
          <button onClick={() => load()} disabled={loading}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100 transition-colors disabled:opacity-50">
            <Icon name="refresh" size={13} />
            刷新
          </button>
          <button onClick={clearAll} disabled={busy || total === 0}
            className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-red-500 text-xs hover:bg-red-50 transition-colors disabled:opacity-40">
            <Icon name="trash" size={13} />
            清空
          </button>
        </div>
      )}

      {loading ? (
        <div className="text-sm text-slate-400">加载中...</div>
      ) : total === 0 ? (
        <div className="text-sm text-slate-400">还没有上传记录。</div>
      ) : (
        <div className="rounded-lg border border-slate-200 bg-white divide-y divide-slate-100">
          {(state?.entries ?? []).map(entry => (
            <div key={entry.id} className="flex items-center gap-2 px-3 py-2 text-xs">
              <Icon name="upload" size={13} className="text-slate-400 flex-shrink-0" />
              <span className="text-slate-500 flex-shrink-0 tabular-nums">{formatTime(entry.at)}</span>
              <span className={`flex-shrink-0 px-1.5 py-0.5 rounded text-[10px] font-medium ${
                entry.username ? 'bg-slate-100 text-slate-600' : 'bg-amber-50 text-amber-700'
              }`}>
                {entry.username || '无账号'}
              </span>
              <span className="text-slate-700 truncate" title={entry.path}>{entry.path || entry.name}</span>
              <span className="ml-auto text-slate-400 flex-shrink-0 tabular-nums">{formatSize(entry.size)}</span>
            </div>
          ))}
        </div>
      )}

      {shown > 0 && shown < total && (
        <button onClick={() => load(shown + PAGE_SIZE)} disabled={loading}
          className="w-full py-2 rounded-lg text-slate-500 text-xs hover:bg-slate-100 transition-colors disabled:opacity-50">
          再显示 {Math.min(PAGE_SIZE, total - shown)} 条（还有 {total - shown} 条）
        </button>
      )}

      <p className="text-[11px] text-slate-400">
        只记录通过本服务上传的文件。用 SMB、rsync 或命令行直接放进来的文件不会出现在这里，
        所以没有记录不代表没人放过。
      </p>
    </div>
  );
}
