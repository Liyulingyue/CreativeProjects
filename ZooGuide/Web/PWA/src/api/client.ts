import type {
  CheckinResponse,
  Meta,
  QuizOptions,
  Route,
  UserPreference,
  Venue,
} from '../types'

const BASE = '' // proxied via vite to backend

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    let msg = `${res.status}`
    try {
      const body = await res.json()
      msg = body.detail || msg
    } catch {
      // ignore
    }
    throw new Error(msg)
  }
  return res.json()
}

export const api = {
  health: () => request<{ status: string; use_llm: boolean; venue_count: number }>('/api/health'),
  meta: () => request<Meta>('/api/meta'),
  quizOptions: () => request<QuizOptions>('/api/quiz-options'),
  venues: () => request<{ venues: Venue[] }>('/api/venues'),
  venue: (id: string) => request<Venue>(`/api/venues/${id}`),
  plan: (prefs: UserPreference) =>
    request<Route>('/api/plan', {
      method: 'POST',
      body: JSON.stringify(prefs),
    }),
  replan: (params: {
    original_route: Route
    current_venue_id?: string
    elapsed_minutes: number
    feedback: string
  }) =>
    request<Route>('/api/replan', {
      method: 'POST',
      body: JSON.stringify(params),
    }),
  checkin: (venue_id: string, session_id?: string) =>
    request<CheckinResponse>('/api/checkin', {
      method: 'POST',
      body: JSON.stringify({ venue_id, session_id }),
    }),
  getCheckins: (session_id: string) =>
    request<{ session_id: string; checkins: any[]; completion_rate: number }>(
      `/api/checkin/${session_id}`,
    ),
}