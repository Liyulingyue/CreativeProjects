import { useEffect, useRef, useState } from 'react'
import type { UserPreference } from '../types'
import { api } from '../api/client'

interface StreamEvent {
  type: 'thinking' | 'token' | 'done' | 'error'
  text?: string
  route?: any
  message?: string
}

/**
 * Stream-based planning. Emits thinking events + token chunks + final route.
 * Falls back to non-streaming if backend doesn't support SSE.
 */
export function useStreamPlan() {
  const [events, setEvents] = useState<StreamEvent[]>([])
  const [route, setRoute] = useState<any | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [done, setDone] = useState(false)
  const startedRef = useRef(false)

  async function start(prefs: UserPreference) {
    if (startedRef.current) return
    startedRef.current = true
    setEvents([])
    setRoute(null)
    setError(null)
    setDone(false)
    try {
      const resp = await fetch('/api/plan-stream', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ...prefs, fast: prefs.fast }),
      })
      if (!resp.ok || !resp.body) {
        // Fallback to non-streaming
        const r = await api.plan(prefs)
        setRoute(r)
        setDone(true)
        return
      }
      const reader = resp.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ''
      while (true) {
        const { value, done: rDone } = await reader.read()
        if (rDone) break
        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split('\n')
        buffer = lines.pop() ?? ''
        let eventName = ''
        let dataStr = ''
        for (const line of lines) {
          if (line.startsWith('event:')) eventName = line.slice(6).trim()
          else if (line.startsWith('data:')) dataStr += line.slice(5).trim()
          else if (line === '') {
            if (eventName && dataStr) {
              try {
                const data = JSON.parse(dataStr)
                if (eventName === 'done' && data.route) {
                  setRoute(data.route)
                  setDone(true)
                } else if (eventName === 'error') {
                  setError(data.message || 'unknown')
                } else {
                  setEvents((prev) => [...prev, { type: eventName as any, text: data.text }])
                }
              } catch {
                // ignore parse errors
              }
            }
            eventName = ''
            dataStr = ''
          }
        }
      }
      if (!done) {
        setDone(true)
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : '规划失败')
      setDone(true)
    } finally {
      startedRef.current = false
    }
  }

  return { events, route, error, done, start }
}