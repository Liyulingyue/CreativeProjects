import type { CityList, CityMatrix, Health, RefreshStatus, SearchResult } from '../types';

const BASE = '/api';
const TOKEN_KEY = 'travelsites_token';
const USER_KEY = 'travelsites_user';

// ----- Auth token 持久化 -----

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string | null): void {
  if (token) {
    localStorage.setItem(TOKEN_KEY, token);
  } else {
    localStorage.removeItem(TOKEN_KEY);
  }
}

export function getUser<T = any>(): T | null {
  const raw = localStorage.getItem(USER_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

export function setUser<T = any>(user: T | null): void {
  if (user) {
    localStorage.setItem(USER_KEY, JSON.stringify(user));
  } else {
    localStorage.removeItem(USER_KEY);
  }
}

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    ...(options?.headers as Record<string, string> || {}),
  };
  const token = getToken();
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  const res = await fetch(`${BASE}${path}`, { ...options, headers });

  if (res.status === 429) {
    const retryAfter = res.headers.get('Retry-After') || '60';
    throw new Error(`请求过于频繁，请 ${retryAfter} 秒后重试`);
  }
  if (res.status === 401) {
    // token 失效，清除本地
    setToken(null);
    setUser(null);
    throw new Error('登录已过期，请重新登录');
  }
  if (!res.ok) {
    const err = await res.json().catch(() => ({ detail: res.statusText }));
    throw new Error(err.detail || err.message || `API Error: ${res.status}`);
  }
  return res.json();
}

export async function logout(): Promise<void> {
  const token = getToken();
  if (!token) return;
  await fetch(`${BASE}/auth/logout`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
  }).catch(() => {});
}

export async function fetchHealth(): Promise<Health> {
  return request<Health>('/health');
}

export async function fetchCities(): Promise<CityList> {
  return request<CityList>('/cities');
}

export async function fetchCityMatrix(city: string): Promise<CityMatrix> {
  return request<CityMatrix>(`/cities/${encodeURIComponent(city)}`);
}

export async function fetchRefreshStatus(): Promise<RefreshStatus> {
  return request<RefreshStatus>('/refresh/status');
}

export async function triggerRefresh(): Promise<{ message: string; cities: string[] }> {
  const res = await fetch(`${BASE}/refresh`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`API Error: ${res.status}`);
  }
  return res.json();
}

interface SearchParams {
  startDate?: string;
  endDate?: string;
  duration?: number;
  style?: 'standard' | 'family' | 'budget';
  sortBy?: string;
  preference?: string;
  origin?: { province: string; city: string; county: string };
}

export async function searchTravelPlans(params: SearchParams): Promise<SearchResult> {
  const body: any = {};
  if (params.startDate) body.start_date = params.startDate;
  if (params.endDate) body.end_date = params.endDate;
  if (params.duration) body.duration = params.duration;
  if (params.style) body.style = params.style;
  if (params.sortBy) body.sort_by = params.sortBy;
  if (params.preference) body.preference = params.preference;
  const origin = params.origin || { province: '北京市', city: '北京市', county: '朝阳区' };
  body.origin_province = origin.province;
  body.origin_city = origin.city;
  body.origin_county = origin.county;

  return request<SearchResult>('/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}
