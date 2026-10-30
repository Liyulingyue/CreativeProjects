import { useCallback, useEffect, useState } from 'react';
import { Icon } from './ui/Icon';

interface Restriction {
  path: string;
  restriction: 'none' | 'read';
}

interface Account {
  id: string;
  username: string;
  role: 'admin' | 'user';
  disabled: boolean;
  created_at: number;
  restrictions: Restriction[];
}

interface UsersState {
  users: Account[];
  count: number;
}

/**
 * The accounts, and the folders that are not for everybody.
 *
 * This is one household's machine: everybody has a key to the same
 * house, and everybody can put things in it and look at what is there.
 * A folder becomes private only when somebody says so, and this is
 * where they say it. A restriction on a folder covers everything under
 * it, and the most specific one for a path is the one that counts --
 * so a restriction on "photos/2024" beats one on "photos".
 *
 * The administrator is not subject to any of this, so there is always
 * a way back in.
 */
export function UsersPanel() {
  const [state, setState] = useState<UsersState | null>(null);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [newUser, setNewUser] = useState({ username: '', password: '', role: 'user' });
  const [restrictFor, setRestrictFor] = useState<Account | null>(null);
  const [restrictPath, setRestrictPath] = useState('');
  const [restrictKind, setRestrictKind] = useState<'none' | 'read'>('none');

  const load = useCallback(async () => {
    try {
      const res = await fetch('/api/users', { headers: authHeaders() });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body?.detail || '加载用户失败');
      }
      setState(await res.json());
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '加载失败');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const call = async (path: string, method: string, body?: unknown) => {
    const res = await fetch(path, {
      method,
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!res.ok) {
      const b = await res.json().catch(() => ({}));
      throw new Error(typeof b?.detail === 'string' ? b.detail : '操作失败');
    }
    return res.json().catch(() => ({}));
  };

  const create = async () => {
    setBusy(true); setMessage(null);
    try {
      await call('/api/users', 'POST', newUser);
      setNewUser({ username: '', password: '', role: 'user' });
      setMessage(`已创建用户 ${newUser.username}`);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '创建失败');
    } finally { setBusy(false); }
  };

  const remove = async (account: Account) => {
    if (!window.confirm(`删除用户「${account.username}」？它的所有权限也会一并删除。`)) return;
    setMessage(null);
    try {
      await call(`/api/users/${account.id}`, 'DELETE');
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '删除失败');
    }
  };

  const toggleDisabled = async (account: Account) => {
    setMessage(null);
    try {
      await call(`/api/users/${account.id}/disabled`, 'POST', { disabled: !account.disabled });
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '操作失败');
    }
  };

  const addRestriction = async () => {
    if (!restrictFor) return;
    setMessage(null);
    try {
      await call('/api/users/restrictions', 'POST', {
        path: restrictPath.trim(),
        user_id: restrictFor.id,
        restriction: restrictKind,
      });
      setRestrictPath('');
      setRestrictFor(null);
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '设置失败');
    }
  };

  const removeRestriction = async (account: Account, restriction: Restriction) => {
    setMessage(null);
    try {
      await call(
        `/api/users/restrictions?path=${encodeURIComponent(restriction.path)}&user_id=${account.id}`,
        'DELETE',
      );
      await load();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : '解除失败');
    }
  };

  return (
    <div className="space-y-4">
      {message && (
        <div className="text-sm text-indigo-700 bg-indigo-50 rounded-lg px-3 py-2">{message}</div>
      )}

      {loading ? (
        <div className="text-sm text-slate-400">加载中...</div>
      ) : (
        <>
          <div className="space-y-2">
            {(state?.users ?? []).map(account => (
              <div key={account.id} className="rounded-lg border border-slate-200 bg-white p-3">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="text-sm font-medium text-slate-800">{account.username}</span>
                  {account.role === 'admin' && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-indigo-100 text-indigo-700 font-medium">管理员</span>
                  )}
                  {account.disabled && (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-slate-200 text-slate-600 font-medium">已停用</span>
                  )}
                  <div className="flex-1" />
                  {account.role !== 'admin' && (
                    <>
                      <button onClick={() => toggleDisabled(account)}
                        className="px-2.5 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100 transition-colors">
                        {account.disabled ? '启用' : '停用'}
                      </button>
                      <button onClick={() => remove(account)}
                        className="px-2.5 py-1.5 rounded-lg text-red-500 text-xs hover:bg-red-50 transition-colors">
                        删除
                      </button>
                    </>
                  )}
                </div>

                {account.role === 'admin' ? (
                  <div className="text-[11px] text-slate-400 mt-1.5">管理员可以访问全部文件</div>
                ) : (
                  <div className="mt-2 space-y-1.5">
                    {account.restrictions.length === 0 && (
                      <div className="text-[11px] text-slate-400">
                        能访问全部文件，和其他人一样
                      </div>
                    )}
                    {account.restrictions.map(restriction => (
                      <div key={`${restriction.path}-${restriction.restriction}`}
                        className="flex items-center gap-2 text-xs">
                        <Icon name="folder" size={13} className="text-slate-400 flex-shrink-0" />
                        <span className="text-slate-600 truncate">
                          {restriction.path || '根目录'}
                        </span>
                        <span className={`px-1.5 py-0.5 rounded text-[10px] font-medium flex-shrink-0 ${
                          restriction.restriction === 'none'
                            ? 'bg-red-100 text-red-700'
                            : 'bg-slate-100 text-slate-600'
                        }`}>
                          {restriction.restriction === 'none' ? '看不到' : '只读'}
                        </span>
                        <button onClick={() => removeRestriction(account, restriction)}
                          className="text-indigo-500 hover:text-indigo-700 text-[11px] ml-auto flex-shrink-0">
                          解除
                        </button>
                      </div>
                    ))}
                    {restrictFor?.id === account.id ? (
                      <div className="flex items-center gap-2 pt-1 flex-wrap">
                        <input
                          value={restrictPath}
                          onChange={e => setRestrictPath(e.target.value)}
                          onKeyDown={e => { if (e.key === 'Enter') addRestriction(); }}
                          placeholder="目录路径"
                          className="flex-1 min-w-[10rem] px-2.5 py-1.5 rounded-lg border border-slate-200 bg-slate-50 text-xs focus:outline-none focus:ring-2 focus:ring-indigo-400/30"
                        />
                        <select value={restrictKind} onChange={e => setRestrictKind(e.target.value as 'none' | 'read')}
                          className="px-2 py-1.5 rounded-lg border border-slate-200 bg-slate-50 text-xs">
                          <option value="none">看不到</option>
                          <option value="read">只读</option>
                        </select>
                        <button onClick={addRestriction}
                          className="px-2.5 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 transition-colors">
                          设置
                        </button>
                        <button onClick={() => setRestrictFor(null)}
                          className="px-2 py-1.5 rounded-lg text-slate-500 text-xs hover:bg-slate-100 transition-colors">
                          取消
                        </button>
                      </div>
                    ) : (
                      <button onClick={() => { setRestrictFor(account); setRestrictPath(''); }}
                        className="text-[11px] text-indigo-600 hover:underline">
                        + 把某个目录设为私密
                      </button>
                    )}
                  </div>
                )}
              </div>
            ))}
          </div>

          <div className="rounded-lg border border-dashed border-slate-300 bg-slate-50/60 p-3 space-y-2">
            <div className="text-xs font-medium text-slate-600">新建用户</div>
            <div className="flex flex-wrap gap-2">
              <input value={newUser.username} onChange={e => setNewUser({ ...newUser, username: e.target.value })}
                placeholder="用户名"
                className="flex-1 min-w-[8rem] px-2.5 py-1.5 rounded-lg border border-slate-200 bg-white text-xs focus:outline-none focus:ring-2 focus:ring-indigo-400/30" />
              <input type="password" value={newUser.password} onChange={e => setNewUser({ ...newUser, password: e.target.value })}
                placeholder="密码（至少 4 位）"
                className="flex-1 min-w-[8rem] px-2.5 py-1.5 rounded-lg border border-slate-200 bg-white text-xs focus:outline-none focus:ring-2 focus:ring-indigo-400/30" />
              <select value={newUser.role} onChange={e => setNewUser({ ...newUser, role: e.target.value })}
                className="px-2 py-1.5 rounded-lg border border-slate-200 bg-white text-xs">
                <option value="user">普通用户</option>
                <option value="admin">管理员</option>
              </select>
              <button onClick={create} disabled={busy || !newUser.username || newUser.password.length < 4}
                className="px-3 py-1.5 rounded-lg bg-indigo-600 text-white text-xs font-medium hover:bg-indigo-700 disabled:opacity-40 transition-colors">
                {busy ? '创建中...' : '创建'}
              </button>
            </div>
            <div className="text-[11px] text-slate-400">
              默认所有人都能看、能传，和一家人共用一个柜子一样。
              某个人的私密目录，在这里单独设。
            </div>
          </div>
        </>
      )}
    </div>
  );
}

function authHeaders(): Record<string, string> {
  const token = localStorage.getItem('sfm_token');
  return token ? { Authorization: `Bearer ${token}` } : {};
}
