import { useCallback, useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import {
  fetchPublicShare, fetchPublicShareListing, unlockPublicShare,
  type PublicShareInfo,
} from '../api';
import { Icon } from './ui/Icon';

interface ListingItem {
  name: string;
  is_dir: boolean;
  size: number;
  modified: string;
}

function formatSize(bytes: number): string {
  if (!bytes) return '';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
}

/**
 * What somebody sees when they open a share link.
 *
 * No login, no token, no navigation of the service around it -- just
 * the one thing that was shared. The page is deliberately plain: a
 * visitor who followed a link should not be able to tell what else
 * this NAS holds.
 */
export function PublicSharePage() {
  const { id = '' } = useParams();
  const [info, setInfo] = useState<PublicShareInfo | null>(null);
  const [items, setItems] = useState<ListingItem[] | null>(null);
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [unlocking, setUnlocking] = useState(false);

  const load = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      const data = await fetchPublicShare(id);
      setInfo(data);
      if (!data.locked && data.is_dir) {
        const listing = await fetchPublicShareListing(id);
        setItems(listing.items);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : '链接无效');
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => { load(); }, [load]);

  const unlock = async () => {
    setUnlocking(true); setError(null);
    try {
      await unlockPublicShare(id, password);
      setPassword('');
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : '密码不正确');
    } finally {
      setUnlocking(false);
    }
  };

  const downloadUrl = (name?: string) => {
    const base = `/api/share/${encodeURIComponent(id)}/download`;
    return name ? `${base}?name=${encodeURIComponent(name)}` : base;
  };

  return (
    <div className="min-h-full bg-slate-50 flex items-start justify-center p-4 sm:p-8">
      <div className="w-full max-w-2xl">
        <div className="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden">
          <div className="px-5 py-4 border-b border-slate-100 flex items-center gap-3">
            <Icon name="share" size={20} className="text-indigo-500 flex-shrink-0" />
            <div className="min-w-0">
              <div className="text-sm font-semibold text-slate-800 truncate">{info?.name ?? '共享文件'}</div>
              {info && !info.is_dir && info.size > 0 && (
                <div className="text-xs text-slate-400">{formatSize(info.size)}</div>
              )}
            </div>
          </div>

          <div className="p-5">
            {loading ? (
              <div className="flex items-center justify-center py-10 text-slate-400">
                <div className="loading-spinner mr-3" /><span>加载中...</span>
              </div>
            ) : error ? (
              <div className="text-center py-10">
                <Icon name="alert" size={36} className="text-slate-300 mb-3" />
                <div className="text-sm text-slate-600">{error}</div>
              </div>
            ) : info?.locked ? (
              <div className="py-6">
                <div className="text-sm text-slate-600 mb-3">这个链接需要密码。</div>
                <div className="flex gap-2">
                  <input
                    type="password"
                    value={password}
                    onChange={e => setPassword(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') unlock(); }}
                    placeholder="密码"
                    autoFocus
                    className="flex-1 px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
                  />
                  <button onClick={unlock} disabled={unlocking}
                    className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors flex-shrink-0">
                    {unlocking ? '验证中...' : '打开'}
                  </button>
                </div>
              </div>
            ) : info?.is_dir ? (
              <>
                <div className="text-xs text-slate-400 mb-3">共 {items?.length ?? 0} 项</div>
                <div className="border border-slate-200 rounded-lg overflow-hidden">
                  {(items ?? []).map(item => (
                    <div key={item.name}
                      className="flex items-center gap-3 px-3 py-2.5 border-b border-slate-100 last:border-b-0">
                      <Icon name={item.is_dir ? 'folder' : 'file'} size={18}
                        className={item.is_dir ? 'text-amber-500' : 'text-slate-400'} />
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-slate-700 truncate">{item.name}</div>
                        <div className="text-[11px] text-slate-400">
                          {item.is_dir ? '文件夹' : formatSize(item.size)} · {item.modified}
                        </div>
                      </div>
                      {!item.is_dir && (
                        <a href={downloadUrl(item.name)} download
                          className="px-2.5 py-1.5 rounded-lg bg-slate-100 text-slate-600 text-xs font-medium hover:bg-slate-200 transition-colors flex-shrink-0">
                          下载
                        </a>
                      )}
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <div className="text-center py-8">
                <Icon name="file" size={44} className="text-slate-300 mb-3" />
                <a href={downloadUrl()} download
                  className="inline-flex items-center gap-2 px-5 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
                  <Icon name="download" size={16} />下载文件
                </a>
              </div>
            )}
          </div>
        </div>
        <div className="text-center text-xs text-slate-400 mt-4">通过共享链接访问</div>
      </div>
    </div>
  );
}
