const TOKEN_KEY = 'sfm_token';
const AUTH_ENABLED_KEY = 'sfm_auth_enabled';

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string) {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken() {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(AUTH_ENABLED_KEY);
}

export function getAuthEnabled(): boolean {
  return localStorage.getItem(AUTH_ENABLED_KEY) === 'true';
}

export function setAuthEnabled(enabled: boolean) {
  localStorage.setItem(AUTH_ENABLED_KEY, String(enabled));
}

export function authHeaders(): Record<string, string> {
  const token = getToken();
  if (token) {
    return { 'Authorization': `Bearer ${token}` };
  }
  return {};
}

/**
 * Fired on window when authFetch sees a 401. App listens for this and
 * flips into the unauthenticated state (showing LoginPage) instead of
 * doing a hard window.location.reload(), which restarts the whole
 * React app on every API failure.
 */
export const AUTH_LOGOUT_EVENT = 'sfm:auth-logout';

export function onAuthLogout(handler: () => void): () => void {
  const wrapped = () => handler();
  window.addEventListener(AUTH_LOGOUT_EVENT, wrapped);
  return () => window.removeEventListener(AUTH_LOGOUT_EVENT, wrapped);
}

export async function authFetch(url: string, options: RequestInit = {}): Promise<Response> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string> || {}),
    ...authHeaders(),
  };
  if (options.body && typeof options.body === 'string' && !headers['Content-Type']) {
    headers['Content-Type'] = 'application/json';
  }
  const res = await fetch(url, { ...options, headers });
  if (res.status === 401) {
    clearToken();
    // Dispatch a logout event so App can swap to LoginPage in place
    // instead of reloading the entire SPA. Multiple concurrent 401s
    // would otherwise trigger multiple reloads; the event model is
    // idempotent — handlers just set a state flag.
    window.dispatchEvent(new CustomEvent(AUTH_LOGOUT_EVENT));
    throw new Error('登录已过期，请重新登录');
  }
  return res;
}

// ---- Auth API ----

export async function checkAuthStatus(): Promise<{ requires_auth: boolean; has_password: boolean }> {
  const res = await fetch('/api/auth/status');
  if (!res.ok) throw new Error('读取登录状态失败');
  return res.json();
}

export async function login(
  password: string,
  username = '',
): Promise<{ token: string; requires_auth: boolean; username: string; role: string }> {
  const res = await fetch('/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password, username }),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data.detail || '登录失败');
  }
  return res.json();
}

export async function verifyToken(): Promise<{ valid: boolean; requires_auth: boolean }> {
  const res = await fetch('/api/auth/verify', {
    headers: authHeaders(),
  });
  if (!res.ok) return { valid: false, requires_auth: true };
  return res.json();
}

export async function logout(): Promise<void> {
  try {
    await fetch('/api/auth/logout', { method: 'POST', headers: authHeaders() });
  } catch {
    // ignore
  }
  clearToken();
}

export async function changePassword(oldPassword: string, newPassword: string): Promise<{ success: boolean; token?: string; message?: string }> {
  const res = await fetch('/api/auth/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ old_password: oldPassword, new_password: newPassword }),
  });
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data.detail || '操作失败');
  }
  return res.json();
}
