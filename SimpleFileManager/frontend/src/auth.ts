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
    window.location.hash = '#/login';
    window.location.reload();
    throw new Error('Unauthorized');
  }
  return res;
}

// ---- Auth API ----

export async function checkAuthStatus(): Promise<{ requires_auth: boolean; has_password: boolean }> {
  const res = await fetch('/api/auth/status');
  if (!res.ok) throw new Error('Failed to check auth status');
  return res.json();
}

export async function login(password: string): Promise<{ token: string; requires_auth: boolean }> {
  const res = await fetch('/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password }),
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
