import { useState, useEffect } from 'react';
import { checkAuthStatus, login, setToken, setAuthEnabled, getAuthEnabled } from '../auth';
import { Icon } from './ui/Icon';

interface LoginPageProps {
  onLogin: () => void;
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [needsPassword, setNeedsPassword] = useState(false);

  useEffect(() => {
    checkAuthStatus()
      .then(status => {
        setNeedsPassword(status.requires_auth);
        setAuthEnabled(status.requires_auth);
        if (!status.requires_auth) {
          onLogin();
        }
      })
      .catch(() => {
        setNeedsPassword(false);
        onLogin();
      });
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!password.trim() || loading) return;
    setLoading(true); setError(null);
    try {
      const result = await login(password);
      setToken(result.token);
      setAuthEnabled(result.requires_auth);
      onLogin();
    } catch (err) {
      setError(err instanceof Error ? err.message : '登录失败');
    } finally {
      setLoading(false);
    }
  };

  if (!needsPassword && !getAuthEnabled()) return null;

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-slate-50">
      <div className="w-full max-w-sm mx-3 sm:mx-4">
        <div className="bg-white rounded-xl shadow-popover border border-slate-200 p-8">
          {/* Logo */}
          <div className="flex flex-col items-center mb-6">
            <div className="w-12 h-12 rounded-xl bg-indigo-600 flex items-center justify-center text-white mb-3">
              <Icon name="folder" size={28} />
            </div>
            <h1 className="text-lg font-semibold text-slate-800">SimpleFileManager</h1>
            <p className="text-sm text-slate-400 mt-1">输入密码以继续</p>
          </div>

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <input
                type="password"
                value={password}
                onChange={e => setPassword(e.target.value)}
                autoFocus
                placeholder="密码"
                className="w-full px-4 py-2.5 rounded-lg border border-slate-200 bg-slate-50 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-400/30 focus:border-indigo-400 transition-all"
              />
            </div>

            {error && (
              <div className="text-sm text-red-600 bg-red-50 rounded-lg px-3 py-2">{error}</div>
            )}

            <button
              type="submit"
              disabled={loading || !password.trim()}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              {loading ? '验证中...' : '登录'}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
