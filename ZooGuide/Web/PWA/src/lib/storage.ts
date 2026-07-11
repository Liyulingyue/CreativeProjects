import type { UserPreference } from '../types'

const KEY = 'zooguide:prefs:v1'

export function loadPrefs(): UserPreference | null {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return null
    return JSON.parse(raw)
  } catch {
    return null
  }
}

export function savePrefs(prefs: UserPreference) {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs))
  } catch {
    // ignore
  }
}

const SESSION_KEY = 'zooguide:session:v1'
export function getSessionId(): string {
  let sid = localStorage.getItem(SESSION_KEY)
  if (!sid) {
    sid = crypto.randomUUID()
    localStorage.setItem(SESSION_KEY, sid)
  }
  return sid
}

const VISITED_KEY = 'zooguide:visited:v1'

export function loadVisited(): Set<string> {
  try {
    const raw = localStorage.getItem(VISITED_KEY)
    return new Set(raw ? JSON.parse(raw) : [])
  } catch {
    return new Set()
  }
}

export function saveVisited(ids: Set<string>) {
  localStorage.setItem(VISITED_KEY, JSON.stringify([...ids]))
  window.dispatchEvent(new Event('zooguide:visitedChanged'))
}

const ACTIVITY_VISITED_PREFIX = 'zooguide:activity:visited:'

export function loadActivityVisited(activity: string): Set<string> {
  try {
    const raw = localStorage.getItem(ACTIVITY_VISITED_PREFIX + activity)
    return new Set(raw ? JSON.parse(raw) : [])
  } catch {
    return new Set()
  }
}

export function saveActivityVisited(activity: string, ids: Set<string>) {
  localStorage.setItem(ACTIVITY_VISITED_PREFIX + activity, JSON.stringify([...ids]))
  window.dispatchEvent(new Event('zooguide:activityVisitedChanged'))
}

// Recent photo evaluations (capped)
const PHOTO_LOG_KEY = 'zooguide:photoLog:v1'
const PHOTO_LOG_MAX = 30

export interface PhotoLogEntry {
  evaluation_id: string
  animal_guess: string
  matched_venue_id: string
  matched_venue_name: string
  badge: string
  vibe_score: number
  caption: string
  ts: string
}

export function loadPhotoLog(): PhotoLogEntry[] {
  try {
    const raw = localStorage.getItem(PHOTO_LOG_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

export function appendPhotoLog(entry: PhotoLogEntry) {
  const log = loadPhotoLog()
  log.unshift(entry) // newest first
  if (log.length > PHOTO_LOG_MAX) log.pop()
  try {
    localStorage.setItem(PHOTO_LOG_KEY, JSON.stringify(log))
  } catch {}
  window.dispatchEvent(new Event('zooguide:photoLogChanged'))
}

export function clearPhotoLog() {
  localStorage.removeItem(PHOTO_LOG_KEY)
  window.dispatchEvent(new Event('zooguide:photoLogChanged'))
}

const TOKEN_KEY = 'zooguide:token:v1'
const USER_KEY = 'zooguide:user:v1'

export interface AuthUser {
  id: number
  username: string
  display_name: string
}

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function setAuth(token: string, user: AuthUser) {
  localStorage.setItem(TOKEN_KEY, token)
  localStorage.setItem(USER_KEY, JSON.stringify(user))
}

export function clearAuth() {
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(USER_KEY)
}

export function getStoredUser(): AuthUser | null {
  try {
    const raw = localStorage.getItem(USER_KEY)
    return raw ? JSON.parse(raw) : null
  } catch {
    return null
  }
}