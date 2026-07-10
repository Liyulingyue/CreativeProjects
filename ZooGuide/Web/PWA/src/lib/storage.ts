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
    if (!raw) return new Set()
    return new Set(JSON.parse(raw))
  } catch {
    return new Set()
  }
}

export function saveVisited(ids: Set<string>) {
  localStorage.setItem(VISITED_KEY, JSON.stringify([...ids]))
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