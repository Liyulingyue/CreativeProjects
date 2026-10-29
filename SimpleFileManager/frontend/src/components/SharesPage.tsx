import { useCallback, useEffect, useState } from 'react';
import { fetchShares, revokeShare, setShareExpiry, type Share } from '../api';
import { Icon } from './ui/Icon';
import { formatExpiry } from './ShareDialog';

function formatSize(bytes: number): string {
  if (!bytes) return '';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

/**
 * The shares that are currently open.
 *
 * A share is a door left open, so the list says plainly which ones
 * are still working, which have stopped, and how much longer each
 * one lasts. Stopping one is one click.
 */
export function SharesPage() {
  const [shares, setShares] = useState<Share[]>([]);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await fetchShares();
      setShares(data.shares);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const copy = async (share: Share) => {
    const url = `${window.location.origin}/s/${share.id}`;
    try {
      await navigator.clipboard.writeText(url);
      setCopied(share.id);
      setTimeout(() => setCopied(null), 2000);
    } catch {
      setMessage('复制失败');
    }
  };

  const revoke = async (share: Share) => {
    if (!window.confirm(`停止分享「${share.name}」？链接会立刻失效。`)) return;
    try {
      await revokeShare(share.id);
      setMessage(`已停止分享「${share.name}」`);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    }
  };

  const extend = async (share: Share, days: number) => {
    try {
      await setShareExpiry(share.id, days);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    }
  };

  const active = shares.filter(s => s.active);
  const inactive = shares.filter(s => !s.active);

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-3 flex-shrink-0">
        <Icon name="share" size={20} className="text-indigo-600" />
        <span className="text-base font-semibold text-slate-800">共享链接</span>
        <span className="hidden sm:block text-xs text-slate-400">
          {active.length > 0 ? `${active.length} 个链接可用` : '还没有生效的链接'}
        </span>
      </div>

      {message && <div className="px-4 sm:px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {loading ? (
          <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>
        ) : shares.length === 0 ? (
          <div className="text-center py-16 text-slate-400">
            <Icon name="share" size={40} className="mx-auto text-slate-300 mb-3" />
            <div className="text-base">还没有共享链接</div>
            <div className="text-sm mt-1">在文件上右键 →「分享」即可创建一个只读链接</div>
          </div>
        ) : (
          <div className="space-y-4">
            {active.length > 0 && (
              <div className="bg-white rounded-lg border border-slate-200 overflow-hidden">
                <div className="px-4 py-2.5 bg-slate-50 border-b border-slate-100 text-xs font-medium text-slate-500">
                  生效中
                </div>
                {active.map(share => (
                  <ShareRow key={share.id} share={share} onCopy={copy} onRevoke={revoke} onExtend={extend} copied={copied === share.id} />
                ))}
              </div>
            )}
            {inactive.length > 0 && (
              <div className="bg-white rounded-lg border border-slate-200 overflow-hidden opacity-70">
                <div className="px-4 py-2.5 bg-slate-50 border-b border-slate-100 text-xs font-medium text-slate-500">
                  已失效
                </div>
                {inactive.map(share => (
                  <ShareRow key={share.id} share={share} onCopy={copy} onRevoke={revoke} onExtend={extend} copied={copied === share.id} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ShareRow({ share, onCopy, onRevoke, onExtend, copied }: {
  share: Share;
  onCopy: (s: Share) => void;
  onRevoke: (s: Share) => void;
  onExtend: (s: Share, days: number) => void;
  copied: boolean;
}) {
  const status = share.revoked ? '已停止' : share.expired ? '已过期' : share.used_up ? '次数用完' : formatExpiry(share.expires_at);
  return (
    <div className="flex items-center gap-3 px-4 py-3 border-b border-slate-100 last:border-b-0">
      <Icon name={share.is_dir ? 'folder' : 'file'} size={18}
        className={share.is_dir ? 'text-amber-500' : 'text-slate-400'} />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium text-slate-800 truncate">{share.name}</div>
        <div className="text-[11px] text-slate-400 truncate">
          {share.is_dir ? '文件夹' : formatSize(share.size)}
          {share.has_password && ' · 有密码'}
          {share.max_downloads > 0 && ` · 限 ${share.downloads}/${share.max_downloads} 次`}
          {share.downloads > 0 && !share.max_downloads && ` · 已访问 ${share.downloads} 次`}
          {' · '}{status}
        </div>
      </div>
      <div className="flex items-center gap-1 flex-shrink-0">
        <button onClick={() => onCopy(share)}
          className="px-2.5 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors">
          {copied ? '已复制' : '复制链接'}
        </button>
        {share.active && (
          <>
            <button onClick={() => onExtend(share, 7)}
              className="px-2.5 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100 transition-colors">
              延长 7 天
            </button>
            <button onClick={() => onRevoke(share)}
              className="px-2.5 py-1.5 rounded-lg text-red-500 text-xs hover:bg-red-50 transition-colors">
              停止
            </button>
          </>
        )}
      </div>
    </div>
  );
}
