import { useState } from 'react'
import type { Route, RouteStop, UserPreference } from '../types'
import { saveVisited } from '../lib/storage'
import { CurrentRouteTab } from './route-tabs/CurrentRouteTab'
import { MoreRoutesTab } from './route-tabs/MoreRoutesTab'
import { AdjustRouteTab } from './route-tabs/AdjustRouteTab'
import { useVisitedVenues } from '../hooks/useVisitedVenues'

interface Props {
  route: Route
  prefs: UserPreference
  onRouteUpdate: (r: Route) => void
  onRestartQuiz?: () => void
  onOpenChat?: () => void
}

type SubTab = 'current' | 'more' | 'adjust'

export function RouteView({
  route,
  prefs,
  onRouteUpdate,
  onRestartQuiz,
  onOpenChat,
}: Props) {
  const [currentStopIdx, setCurrentStopIdx] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(`zooguide:currentStop:${route.id}`)
      if (saved) {
        const idx = parseInt(saved, 10)
        if (!isNaN(idx) && idx >= 0 && idx < route.stops.length) return idx
      }
    } catch {}
    return 0
  })
  const [subTab, setSubTab] = useState<SubTab>('current')
  const { visited } = useVisitedVenues()

  function persistCurrentStop(idx: number) {
    setCurrentStopIdx(idx)
    try {
      localStorage.setItem(`zooguide:currentStop:${route.id}`, String(idx))
    } catch {}
  }

  function toggleVisited(venueId: string) {
    const next = new Set(visited)
    if (next.has(venueId)) next.delete(venueId)
    else next.add(venueId)
    saveVisited(next)
  }

  function openStop(idx: number) {
    persistCurrentStop(idx)
  }

  function elapsedFor(idx: number): number {
    let total = 0
    for (let i = 0; i < idx && i < route.stops.length; i++) {
      total += route.stops[i].visit_minutes + route.stops[i].walk_to_next_minutes
    }
    return total
  }

  return (
    <div className="route-view">
      {subTab === 'current' && (
        <CurrentRouteTab
          route={route}
          prefs={prefs}
          currentStopIdx={currentStopIdx}
          onMarkCurrent={persistCurrentStop}
          onToggleVisited={toggleVisited}
        />
      )}

      {subTab === 'more' && (
        <MoreRoutesTab
          prefs={prefs}
          currentRoute={route}
          onApplyVariant={(r) => {
            onRouteUpdate(r)
            setSubTab('current')
            setCurrentStopIdx(0)
            try {
              localStorage.setItem(`zooguide:currentStop:${r.id}`, '0')
            } catch {}
          }}
        />
      )}

      {subTab === 'adjust' && (
        <AdjustRouteTab
          currentRoute={route}
          currentStopIdx={currentStopIdx}
          elapsedMinutes={elapsedFor(currentStopIdx)}
          prefs={prefs}
          onReplanned={(r) => {
            onRouteUpdate(r)
            setSubTab('current')
            // keep currentStopIdx pointing to similar venue
          }}
          onRestartQuiz={() => onRestartQuiz?.()}
          onOpenChat={() => onOpenChat?.()}
        />
      )}

      {/* 3-tab bottom toolbar */}
      <nav className="route-toolbar">
        <button
          className={`rt-btn ${subTab === 'current' ? 'on' : ''}`}
          onClick={() => setSubTab('current')}
        >
          <span className="rt-icon">📍</span>
          <span className="rt-label">当前</span>
        </button>
        <button
          className={`rt-btn ${subTab === 'more' ? 'on' : ''}`}
          onClick={() => setSubTab('more')}
        >
          <span className="rt-icon">🧭</span>
          <span className="rt-label">更多</span>
        </button>
        <button
          className={`rt-btn ${subTab === 'adjust' ? 'on' : ''}`}
          onClick={() => setSubTab('adjust')}
        >
          <span className="rt-icon">✨</span>
          <span className="rt-label">调整</span>
        </button>
      </nav>
    </div>
  )
}

function loadVisited(): Set<string> {
  try {
    const raw = localStorage.getItem('zooguide:visited:v1')
    return new Set(raw ? JSON.parse(raw) : [])
  } catch {
    return new Set()
  }
}