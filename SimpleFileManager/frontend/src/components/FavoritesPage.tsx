import { useCallback, useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { fetchFavorites, removeFavorite, type Favorite } from '../api';
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
 * The files this person starred.
 *
 * A star in the corner of a file, and a place to come back to. It is
 * one person's list -- "the ones I want to find again" is a private
 * sort of thing -- even on a machine the whole family uses.
 *
 * A starred file that has since been moved or deleted is not shown:
 * there is nothing to open. The star is still there, so if the file
 * comes back it is starred again.
 */
export function FavoritesPage() {
  const [items, setItems] = useState<Favorite[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const navigate = useNavigate();

  const load = useCallback(async () => {
    try {
      const data = await fetchFavorites();
      setItems(data.favorites);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const unstar = async (item: Favorite) => {
    try {
      await removeFavorite(item.path);
      setItems(prev => prev.filter(f => f.path !== item.path));
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '取消收藏失败');
    }
  };

  return (
    <div className="flex flex-col h-full bg-slate-50">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 flex items-center gap-3 flex-shrink-0">
        <Icon name="star" size={20} className="text-amber-500 flex-shrink-0" />
        <span className="text-base font-semibold text-slate-800 flex-shrink-0">我的收藏</span>
        <span className="hidden sm:block text-xs text-slate-400 truncate">
          {isLoading ? '加载中...' : items.length === 0
            ? '右键一个文件就能收藏它'
            : `${items.length} 项`}
        </span>
      </div>

      {message && <div className="px-4 sm:px-6 py-2 bg-indigo-50 text-indigo-700 text-sm">{message}</div>}

      <div className="flex-1 overflow-y-auto p-4 sm:p-6">
        {isLoading ? (
          <div className="flex items-center justify-center h-full text-slate-400">加载中...</div>
        ) : items.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-slate-400 gap-3">
            <Icon name="star" size={40} className="text-slate-200" />
            <div className="text-sm">还没有收藏任何文件</div>
            <div className="text-xs text-slate-400">在文件上点右键，选「收藏」</div>
          </div>
        ) : (
          <div className="max-w-3xl space-y-1.5">
            {items.map(item => (
              <div key={item.path}
                className="flex items-center gap-3 px-3 py-2.5 bg-white rounded-lg border border-slate-200 hover:border-slate-300 transition-colors">
                <Icon name={item.is_dir ? 'folder' : 'file'} size={18}
                  className={item.is_dir ? 'text-amber-500' : 'text-slate-400'} />
                <button onClick={() => navigate(item.is_dir ? `/?path=${encodeURIComponent(item.path)}` : '/')}
                  className="flex-1 min-w-0 text-left">
                  <div className="text-sm text-slate-700 truncate">{item.name}</div>
                  <div className="text-[11px] text-slate-400 truncate" title={item.path}>
                    {item.path}
                  </div>
                </button>
                <span className="text-[11px] text-slate-400 flex-shrink-0">
                  {item.is_dir ? '文件夹' : formatSize(item.size)}
                </span>
                <button onClick={() => unstar(item)}
                  title="取消收藏"
                  className="px-1.5 py-1 rounded-lg text-amber-400 hover:text-amber-600 hover:bg-amber-50 transition-colors flex-shrink-0">
                  <Icon name="star" size={16} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
