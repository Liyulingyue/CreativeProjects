import type { CityList, CityMatrix, Health, RefreshStatus, SearchResult } from '../types';

const BASE = '/api';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, options);
  if (!res.ok) {
    const err = await res.json().catch(() => ({ detail: res.statusText }));
    throw new Error(err.detail || `API Error: ${res.status}`);
  }
  return res.json();
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

export async function searchTravelPlans(
  startDate: string,
  endDate: string,
  preference: string = '',
  origin: { province: string; city: string; county: string } = { province: '北京市', city: '北京市', county: '朝阳区' }
): Promise<SearchResult> {
  return request<SearchResult>('/search', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      start_date: startDate,
      end_date: endDate,
      preference,
      origin_province: origin.province,
      origin_city: origin.city,
      origin_county: origin.county,
    }),
  });
}
