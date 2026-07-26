import type {
  CheckinResponse,
  Facility,
  Meta,
  NearestResponse,
  PhotoEvaluation,
  QuizOptions,
  Route,
  UserPreference,
  Venue,
} from '../types'

const BASE = '' // proxied via vite to backend

function getToken(): string | null {
  return localStorage.getItem('zooguide:token:v1')
}

export function authHeader(): Record<string, string> {
  const t = getToken()
  return t ? { Authorization: `Bearer ${t}` } : {}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json', ...authHeader() },
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
  facilities: (category?: string) =>
    request<{ facilities: Facility[]; categories: string[] }>(
      `/api/facilities${category ? `?category=${encodeURIComponent(category)}` : ''}`,
    ),
  facility: (id: string) => request<Facility>(`/api/facilities/${id}`),
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
  checkin: (venue_id: string, source?: string, session_id?: string) =>
    request<CheckinResponse>('/api/checkin', {
      method: 'POST',
      body: JSON.stringify({ venue_id, source, session_id }),
    }),
  getCheckins: (session_id: string) =>
    request<{ session_id: string; checkins: any[]; completion_rate: number }>(
      `/api/checkin/${session_id}`,
    ),
  nearest: (lat: number, lon: number, top_k = 3) =>
    request<NearestResponse>(`/api/nearest?lat=${lat}&lon=${lon}&top_k=${top_k}`),
  photoCheckin: async (
    file: File | Blob,
    expectedVenueId: string,
    filename = 'photo.jpg',
    extras?: { thumbnail?: string; preview?: string },
  ) => {
    const form = new FormData()
    form.append('file', file, filename)
    form.append('expected_venue_id', expectedVenueId)
    if (extras?.thumbnail) form.append('thumbnail', extras.thumbnail)
    if (extras?.preview) form.append('preview', extras.preview)
    const res = await fetch(`${BASE}/api/photo-checkin`, {
      method: 'POST',
      body: form,
      headers: authHeader(),
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({}))
      throw new Error(body.detail || `HTTP ${res.status}`)
    }
    return res.json() as Promise<PhotoEvaluation & { success?: boolean; failure_reason?: string }>
  },

  evaluatePhoto: async (
    file: File | Blob,
    filename = 'photo.jpg',
    extras?: { thumbnail?: string; preview?: string },
  ) => {
    const form = new FormData()
    form.append('file', file, filename)
    if (extras?.thumbnail) form.append('thumbnail', extras.thumbnail)
    if (extras?.preview) form.append('preview', extras.preview)
    const res = await fetch(`${BASE}/api/photo-evaluate`, {
      method: 'POST',
      body: form,
      headers: authHeader(),
    })
    if (!res.ok) {
      const body = await res.json().catch(() => ({}))
      throw new Error(body.detail || `HTTP ${res.status}`)
    }
    return res.json() as Promise<PhotoEvaluation & { success?: boolean; failure_reason?: string }>
  },

  // Auth
  register: (username: string, password: string, display_name?: string) =>
    request<{ ok: boolean; token: string; user: { id: number; username: string; display_name: string } }>(
      '/api/auth/register',
      { method: 'POST', body: JSON.stringify({ username, password, display_name }) },
    ),
  login: (username: string, password: string) =>
    request<{ ok: boolean; token: string; user: { id: number; username: string; display_name: string } }>(
      '/api/auth/login',
      { method: 'POST', body: JSON.stringify({ username, password }) },
    ),
  logout: () =>
    request<{ ok: boolean }>('/api/auth/logout', { method: 'POST' }),
  claimSession: (session_id: string) =>
    request<{ ok: boolean; checkins: number; gps_checkins: number; photo_evals: number }>(
      '/api/auth/claim-session',
      { method: 'POST', body: JSON.stringify({ session_id }) },
    ),
  me: () =>
    request<{ id: number; username: string; display_name: string; created_at: string }>('/api/auth/me'),

  // User history
  myVisitedVenueIds: () =>
    request<{ user_id: number; venue_ids: string[] }>('/api/me/visited-venue-ids'),

  sessionPhotoEvals: (sessionId: string) =>
    request<{ session_id: string; evals: any[] }>(`/api/session/photo-evals?session_id=${sessionId}`),

  mySummary: () =>
    request<{
      user: { id: number; username: string; display_name: string }
      stats: {
        checkins_count: number
        venues_visited: number
        photos_evaluated: number
      }
      recent_checkins: Array<{ venue_id: string; venue_name: string; ts: string }>
      recent_photos: Array<{
        evaluation_id: string
        ts: string
        is_match: boolean
        desc: string
        matched_venue_name: string
        score: number
      }>
    }>('/api/me/summary'),

  myAchievements: () =>
    request<{
      user_id: number
      stats: {
        photo_count: number
        checkin_count: number
        venues_unique: number
        best_vibe: number
        consecutive_days: number
        gps_count: number
      }
      achievements: Array<{
        id: string
        name: string
        description: string
        icon: string
        category: string
        criteria_type: string
        criteria_threshold: number
        earned: boolean
        progress: number
        current_value: number
        earned_at: string | null
      }>
      earned_count: number
    }>('/api/me/achievements'),

  // Chat & variants
  chat: (params: {
    message: string
    current_route?: any
    prefs?: any
    history?: any[]
  }) =>
    request<{
      reply: string
      suggested_replan: boolean
      extracted_constraint?: any
      new_route?: any
    }>('/api/chat', { method: 'POST', body: JSON.stringify(params) }),
  planVariants: (prefs: UserPreference) =>
    request<{ variants: Array<any & { variant_label?: string }>; prefs: UserPreference }>(
      '/api/plan-variants',
      { method: 'POST', body: JSON.stringify(prefs) },
    ),
}