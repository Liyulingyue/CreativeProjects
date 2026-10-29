import { useState } from 'react';
import { createShare, type Share } from '../api';
import { Icon } from './ui/Icon';

interface ShareDialogProps {
  path: string;
  name: string;
  isDir: boolean;
  onClose: () => void;
  onCreated: (share: Share) => void;
}

const EXPIRY_OPTIONS = [
  { days: 1, label: '1 天' },
  { days: 7, label: '7 天' },
  { days: 30, label: '30 天' },
  { days: 0, label: '不过期' },
];

/**
 * Creates a share and shows the link.
 *
 * The link is only shown once it is made, and the wording says plainly
 * that anyone with it can open the file -- a share is a door, and the
 * person handing it over should know that.
 */
export function ShareDialog({ path, name, isDir, onClose, onCreated }: ShareDialogProps) {
  const [expiryDays, setExpiryDays] = useState(7);
  const [password, setPassword] = useState('');
  const [maxDownloads, setMaxDownloads] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<Share | null>(null);
  const [copied, setCopied] = useState(false);

  const shareUrl = created
    ? `${window.location.origin}/s/${created.id}`
    : '';

  const submit = async () => {
    setBusy(true); setError(null);
    try {
      const share = await createShare(path, expiryDays, password, maxDownloads);
      setCreated(share);
      onCreated(share);
    } catch (e) {
      setError(e instanceof Error ? e.message : '创建分享失败');
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(shareUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setError('复制失败，请手动选中链接');
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" onClick={onClose}>
      <div className="relative bg-white rounded-xl shadow-popover w-full max-w-lg max-h-[85vh] flex flex-col overflow-hidden" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-3 border-b border-slate-100 flex-shrink-0">
          <div className="flex items-center gap-2 min-w-0">
            <Icon name="link" size={18} className="text-indigo-500 flex-shrink-0" />
            <span className="text-sm font-semibold text-slate-800 truncate">分享「{name}」</span>
          </div>
          <button onClick={onClose} className="w-9 h-9 flex items-center justify-center rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors">
            <Icon name="x" size={18} />
          </button>
        </div>

        <div className="p-5 overflow-y-auto space-y-4">
          {created ? (
            <>
              <div className="text-sm text-slate-600 leading-relaxed">
                链接已经生效。任何拿到它的人都可以打开这个{isDir ? '文件夹' : '文件'}，直到你停用它或它过期。
              </div>
              <div className="flex items-center gap-2">
                <input
                  readOnly
                  value={shareUrl}
                  onFocus={e => e.currentTarget.select()}
                  className="flex-1 px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-xs font-mono text-slate-700"
                />
                <button onClick={copy}
                  className="flex items-center gap-1.5 px-3 py-2 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 transition-colors flex-shrink-0">
                  <Icon name={copied ? 'check' : 'copy'} size={14} />{copied ? '已复制' : '复制'}
                </button>
              </div>
              <div className="text-xs text-slate-400 space-y-1">
                <div>有效期：{created.expires_at ? formatExpiry(created.expires_at) : '不过期'}</div>
                {created.has_password && <div>已设置密码，访问者需要输入后才能打开</div>}
              </div>
              <button onClick={onClose}
                className="w-full px-4 py-2 rounded-lg bg-slate-100 text-slate-600 text-sm font-medium hover:bg-slate-200 transition-colors">
                完成
              </button>
            </>
          ) : (
            <>
              <div className="text-sm text-slate-600 leading-relaxed">
                创建一个只读的链接。任何拿到链接的人都能打开它 —— 只读，但可以下载。
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-500 mb-1.5">有效期</label>
                <div className="flex gap-1.5">
                  {EXPIRY_OPTIONS.map(o => (
                    <button key={o.days} onClick={() => setExpiryDays(o.days)}
                      className={`flex-1 px-2 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                        expiryDays === o.days
                          ? 'bg-indigo-600 text-white'
                          : 'bg-slate-100 text-slate-600 hover:bg-slate-200'
                      }`}>
                      {o.label}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-500 mb-1.5">访问密码（可选）</label>
                <input
                  type="password"
                  value={password}
                  onChange={e => setPassword(e.target.value)}
                  placeholder="留空则不需要密码"
                  className="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
                />
                <div className="text-[11px] text-slate-400 mt-1">
                  链接泄露时，密码是最后一道关
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-slate-500 mb-1.5">下载次数上限（可选）</label>
                <input
                  type="number"
                  min={0}
                  value={maxDownloads || ''}
                  onChange={e => setMaxDownloads(Math.max(0, Number(e.target.value) || 0))}
                  placeholder="0 = 不限"
                  className="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
                />
              </div>

              {error && <div className="text-sm text-red-500">{error}</div>}

              <div className="flex justify-end gap-2 pt-1">
                <button onClick={onClose}
                  className="px-4 py-2 rounded-lg text-sm text-slate-600 hover:bg-slate-100 transition-colors font-medium">
                  取消
                </button>
                <button onClick={submit} disabled={busy}
                  className="px-4 py-2 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors">
                  {busy ? '创建中...' : '创建链接'}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

export function formatExpiry(expiresAt: number): string {
  if (!expiresAt) return '不过期';
  const days = Math.ceil((expiresAt * 1000 - Date.now()) / 86400000);
  if (days <= 0) return '已过期';
  if (days === 1) return '明天到期';
  return `${days} 天后到期`;
}
