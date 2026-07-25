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

const VISITED_KEY = 'zooguide:visited:v2'
const VISITED_KEY_V1 = 'zooguide:visited:v1'

export type VisitSource = 'route' | 'photo' | 'gps'

export interface VisitedMap {
  [venueId: string]: VisitSource[]
}

export function loadVisitedMap(): VisitedMap {
  _migrateV1()
  try {
    const raw = localStorage.getItem(VISITED_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

function _migrateV1() {
  if (!localStorage.getItem(VISITED_KEY_V1)) return
  if (localStorage.getItem(VISITED_KEY)) return
  try {
    const raw = localStorage.getItem(VISITED_KEY_V1)
    if (!raw) return
    const arr: string[] = JSON.parse(raw)
    if (!Array.isArray(arr)) return
    const map: VisitedMap = {}
    for (const id of arr) {
      map[id] = ['route']
    }
    localStorage.setItem(VISITED_KEY, JSON.stringify(map))
    localStorage.removeItem(VISITED_KEY_V1)
  } catch {
    // ignore corrupt v1 data
  }
}

export function saveVisitedMap(map: VisitedMap) {
  localStorage.setItem(VISITED_KEY, JSON.stringify(map))
  window.dispatchEvent(new Event('zooguide:visitedChanged'))
}

export function loadVisited(): Set<string> {
  return new Set(Object.keys(loadVisitedMap()))
}

export function loadVisitedBySource(source: VisitSource): Set<string> {
  const map = loadVisitedMap()
  const ids = new Set<string>()
  for (const [id, sources] of Object.entries(map)) {
    if (sources.includes(source)) ids.add(id)
  }
  return ids
}

export function addVisitedSource(venueId: string, source: VisitSource) {
  const map = loadVisitedMap()
  const sources = map[venueId] || []
  if (!sources.includes(source)) {
    sources.push(source)
    map[venueId] = sources
  }
  saveVisitedMap(map)
}

export function removeVisitedSource(venueId: string, source: VisitSource) {
  const map = loadVisitedMap()
  const sources = map[venueId]
  if (sources) {
    map[venueId] = sources.filter((s) => s !== source)
    if (map[venueId].length === 0) delete map[venueId]
  }
  saveVisitedMap(map)
}

export function removeVisitedSourceAll(source: VisitSource) {
  const map = loadVisitedMap()
  for (const [id, sources] of Object.entries(map)) {
    const filtered = sources.filter((s) => s !== source)
    if (filtered.length === 0) {
      delete map[id]
    } else {
      map[id] = filtered
    }
  }
  saveVisitedMap(map)
}

export function hasVisitedSource(venueId: string, source: VisitSource): boolean {
  const map = loadVisitedMap()
  return (map[venueId] || []).includes(source)
}

export function saveVisited(ids: Set<string>) {
  const map = loadVisitedMap()
  for (const id of ids) {
    if (!map[id]) map[id] = ['route']
  }
  for (const id of Object.keys(map)) {
    if (!ids.has(id) && map[id].length === 1 && map[id][0] === 'route') {
      delete map[id]
    }
  }
  saveVisitedMap(map)
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

export interface ChatMessage {
  role: 'user' | 'assistant'
  content: string
  toolCalls?: { name: string; result: string }[]
  newRoute?: any
}

const CHAT_HISTORY_KEY = 'zooguide:chatHistory:v1'
const CHAT_HISTORY_MAX = 100

export function loadChatHistory(): ChatMessage[] {
  try {
    const raw = localStorage.getItem(CHAT_HISTORY_KEY)
    return raw ? JSON.parse(raw) : []
  } catch {
    return []
  }
}

export function saveChatHistory(messages: ChatMessage[]) {
  try {
    const trimmed = messages.slice(-CHAT_HISTORY_MAX)
    localStorage.setItem(CHAT_HISTORY_KEY, JSON.stringify(trimmed))
  } catch {}
}

export function clearChatHistory() {
  localStorage.removeItem(CHAT_HISTORY_KEY)
}

export function getStoredUser(): AuthUser | null {
  try {
    const raw = localStorage.getItem(USER_KEY)
    return raw ? JSON.parse(raw) : null
  } catch {
    return null
  }
}