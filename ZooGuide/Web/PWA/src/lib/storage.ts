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