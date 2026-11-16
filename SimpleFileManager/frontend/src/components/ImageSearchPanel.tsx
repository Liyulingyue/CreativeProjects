import { useCallback, useEffect, useState } from 'react';
import {
  fetchCurrentUser,
  fetchImageIndexStatus,
  fetchStreamUrl,
  pauseImageIndex,
  resumeImageIndex,
  searchImages,
  searchSimilarImages,
  startImageIndex,
} from '../api';
import type { ImageHit, ImageIndexStatus } from '../api';
import { Icon } from './ui/Icon';
import { useToast } from './ui/Toast';


/**
 * A little picture of a picture, with the file name under it.
 *
 * The server hands out a key for the directory the picture is in, because a
 * picture element cannot send the session token. Without the key the browser
 * would get a 401 and show an empty box, so the name is kept next to it: the
 * search is still useful when the picture itself cannot be drawn.
 *
 * Clicking opens the picture. The link carries a short-lived key for that
 * one file, the same way the media player gets one: downloading the whole
 * picture just to make the tile clickable would pull every result into
 * memory before anyone asked for it.
 */
function HitTile({ hit, onSimilar }: { hit: ImageHit; onSimilar?: (hit: ImageHit) => void }) {
  const [failed, setFailed] = useState(false);
  const [href, setHref] = useState<string | null>(null);
  const src = `/api/thumbnails?path=${encodeURIComponent(hit.path)}&size=256${
    hit.thumbnail_token ? `&tt=${encodeURIComponent(hit.thumbnail_token)}` : ''
  }`;

  useEffect(() => {
    let cancelled = false;
    fetchStreamUrl(hit.path)
      .then(url => { if (!cancelled) setHref(url); })
      .catch(() => { if (!cancelled) setHref(null); });
    return () => { cancelled = true; };
  }, [hit.path]);

  return (
    <div className="group rounded-lg border border-slate-200 bg-white p-2 hover:border-indigo-300 hover:shadow-sm transition-all">
      <a
        href={href ?? undefined}
        target="_blank"
        rel="noreferrer"
        title={hit.path}
        className="block"
      >
        <div className="aspect-square rounded-md bg-slate-100 overflow-hidden flex items-center justify-center">
          {failed ? (
            <Icon name="file" size={28} className="text-slate-300" />
          ) : (
            <img
              src={src}
              alt={hit.name}
              loading="lazy"
              onError={() => setFailed(true)}
              className="w-full h-full object-cover"
            />
          )}
        </div>
        <div className="mt-1.5 text-xs text-slate-600 truncate" title={hit.path}>
          {hit.name}
        </div>
        <div className="text-[10px] text-slate-400">
          {hit.is_source ? '参考图' : `相似度 ${hit.score.toFixed(3)}`}
        </div>
      </a>
      {/* Find the neighbours of this picture. The search is the same one
          the model uses; here it is one click away instead of a sentence
          typed into a chat box. It sits next to the link rather than
          inside it: a button inside a link is two controls in one. */}
      {onSimilar && (
        <button
          type="button"
          onClick={() => onSimilar(hit)}
          className="mt-1 w-full px-2 py-1.5 rounded border border-slate-200 text-[11px] text-slate-600 hover:bg-slate-50 hover:border-indigo-300"
        >
          找相似
        </button>
      )}
    </div>
  );
}

/**
 * The picture index: what is in it, and the button that builds it.
 *
 * Building it is deliberate. A picture costs a few seconds of the server's
 * main cores, and the cores are the same ones the model runs on, so the
 * index is built when the user asks for it rather than in the background.
 */
function IndexBar({ status, onChange, isAdmin }: { status: ImageIndexStatus | null; onChange: () => void; isAdmin: boolean }) {
  const { toast } = useToast();
  const [busy, setBusy] = useState(false);

  // The server says whether it actually did the thing. A build that was
  // already running answers "started: false" -- saying "已开始建立图片
  // 索引" then would be a small lie the user reads straight away.
  const act = async (
    fn: () => Promise<{ started?: boolean; reason?: string }>,
    message: string,
  ) => {
    setBusy(true);
    try {
      const result = await fn();
      onChange();
      if (result?.started === false) {
        toast(result.reason || '操作没有生效', 'error');
      } else {
        toast(message, 'success');
      }
    } catch (err) {
      toast(err instanceof Error ? err.message : '操作失败', 'error');
    } finally {
      setBusy(false);
    }
  };

  if (!status) return null;

  const running = status.running;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-xs text-slate-600">
      <Icon name="image" size={14} className="text-slate-400" />
      <span>
        图片索引 <span className="font-medium text-slate-800">{status.indexed}</span> 张
        {status.total > 0 && running && ` · 本轮 ${status.done}/${status.total}`}
        {status.paused && ' · 已暂停'}
        {status.deferred > 0 && ` · 让路 ${status.deferred}（等大核空闲后重试）`}
        {status.failed > 0 && ` · 失败 ${status.failed}`}
        {(status.not_pictures ?? 0) > 0 && ` · ${status.not_pictures} 个文件不是图片（已跳过）`}
      </span>
      {status.last_error && (
        <span className="text-amber-600 truncate max-w-xs" title={status.last_error}>
          {status.last_error}
        </span>
      )}
      {status.current && <span className="text-slate-400 truncate max-w-xs">正在处理 {status.current}</span>}
      <span className="flex-1" />
      {/* Only an administrator may build the index. The buttons are simply
          not there for anyone else -- a button that always fails is
          worse than no button. */}
      {isAdmin && (running ? (
        <>
          <button
            type="button"
            disabled={busy}
            onClick={() => act(pauseImageIndex, '已暂停，当前这张图处理完就停下')}
            className="px-3 py-2 rounded border border-slate-200 hover:bg-slate-50 disabled:opacity-40"
          >
            暂停
          </button>
          {status.paused && (
            <button
              type="button"
              disabled={busy}
              onClick={() => act(resumeImageIndex, '已继续')}
              className="px-3 py-2 rounded border border-slate-200 hover:bg-slate-50 disabled:opacity-40"
            >
              继续
            </button>
          )}
        </>
      ) : (
        <button
          type="button"
          disabled={busy}
          onClick={() => act(() => startImageIndex(0, false), '已开始建立图片索引')}
          className="px-3 py-2 rounded bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-40"
        >
          建立图片索引
        </button>
      ))}
    </div>
  );
}

/**
 * The picture search: a sentence in, a grid of pictures out.
 *
 * The search needs the server's main cores, and it refuses to queue behind
 * a running model or a running index build. That refusal is shown as it
 * comes back rather than as an empty result, because "no pictures" and
 * "the server was busy" mean very different things to the person asking.
 */
export function ImageSearchPanel() {
  const { toast } = useToast();
  const [query, setQuery] = useState('');
  const [hits, setHits] = useState<ImageHit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<ImageIndexStatus | null>(null);
  // Which kind of search is on the screen. The picture-to-picture one
  // starts from a picture the user picked, so it needs something to go
  // back to.
  const [source, setSource] = useState<ImageHit | null>(null);
  // Building the index is an administrator's job; the server says so.
  const [isAdmin, setIsAdmin] = useState(false);

  const refreshStatus = useCallback(() => {
    fetchImageIndexStatus().then(setStatus).catch(() => setStatus(null));
  }, []);

  useEffect(() => {
    fetchCurrentUser().then(u => setIsAdmin(!!u.is_admin)).catch(() => setIsAdmin(false));
  }, []);

  useEffect(() => {
    refreshStatus();
    const timer = setInterval(refreshStatus, 5000);
    return () => clearInterval(timer);
  }, [refreshStatus]);

  const run = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await searchImages(query.trim(), 24);
      setHits(res.results);
      setSource(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : '未知错误';
      setError(message);
      setHits(null);
      toast(message, 'error');
    } finally {
      setBusy(false);
    }
  };

  // The neighbours of a picture the user picked.
  const findSimilar = async (hit: ImageHit) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await searchSimilarImages(hit.path, 24);
      setHits(res.results);
      setSource(hit);
    } catch (err) {
      const message = err instanceof Error ? err.message : '未知错误';
      setError(message);
      toast(message, 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      <div className="bg-white border-b border-slate-200 px-4 sm:px-6 py-3 sm:py-4 flex-shrink-0 space-y-3">
        <form onSubmit={run} className="flex items-center gap-2 sm:gap-3">
          <div className="relative flex-1 min-w-0">
            <span className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-400">
              <Icon name="image" size={18} />
            </span>
            <input
              type="text"
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder="用一句话描述画面，例如「海边日落」"
              className="w-full pl-10 pr-3 sm:pr-4 py-2 sm:py-2.5 rounded-lg border border-slate-200 bg-slate-50 text-sm sm:text-base focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
            />
          </div>
          <button
            type="submit"
            disabled={busy || !query.trim()}
            className="flex items-center gap-1.5 px-5 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors whitespace-nowrap"
          >
            {busy ? '搜索中...' : '搜图'}
          </button>
        </form>
        <IndexBar status={status} onChange={refreshStatus} isAdmin={isAdmin} />
      </div>

      <div className="flex-1 overflow-y-auto px-4 sm:px-6 py-6">
        {error && (
          <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-700">
            {error}
          </div>
        )}

        {!error && hits === null && (
          <div className="text-center py-16 text-slate-400">
            <Icon name="image" size={40} className="mx-auto text-slate-300 mb-4" />
            <div className="text-base">用文字描述你要找的图片</div>
            <div className="text-sm mt-2">图片检索用的是能同时理解文字和画面的模型，图片需要先建索引</div>
          </div>
        )}

        {!error && hits !== null && hits.length === 0 && (
          <div className="text-center py-16 text-slate-400">
            <div className="text-base">没有找到匹配的图片</div>
            <div className="text-sm mt-2">
              {status && status.running
                // The index is still being built. Saying "try other
                // words" would send the user off looking for a better
                // way to say it, when the pictures simply are not in
                // the index yet.
                ? `图片索引还在建（这轮 ${status.done}/${status.total}），建完再搜`
                : status && status.indexed === 0
                  ? (isAdmin
                    ? '图片索引还是空的，点上面的「建立图片索引」先把图片收进来'
                    : '图片索引还是空的，请管理员在设置里建立图片索引')
                  : '换个说法试试'}
            </div>
          </div>
        )}

        {!error && hits !== null && hits.length > 0 && (
          <>
            {source && (
              <div className="mb-3 flex items-center gap-2 rounded-lg border border-indigo-200 bg-indigo-50 px-3 py-2 text-xs text-indigo-700">
                <Icon name="image" size={14} />
                <span className="truncate">
                  以图搜图：和 <span className="font-medium">{source.name}</span> 最像的 {hits.length} 张
                </span>
                <span className="flex-1" />
                <button
                  type="button"
                  onClick={() => { setSource(null); setHits(null); }}
                  className="px-2 py-1 rounded border border-indigo-200 bg-white hover:bg-indigo-50"
                >
                  回到文搜图
                </button>
              </div>
            )}
            <div className="text-sm text-slate-500 mb-3">
              找到 {hits.length} 张，按相似度排序
              {status?.running && ' · 索引还在建，结果可能不全'}
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-3">
              {hits.map(hit => (
                <HitTile key={hit.path} hit={hit} onSimilar={findSimilar} />
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
