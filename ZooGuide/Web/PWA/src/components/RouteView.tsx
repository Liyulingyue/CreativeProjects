import { useState } from 'react'
import type { Route, RouteStop, UserPreference } from '../types'
import { addVisitedSource, removeVisitedSource } from '../lib/storage'
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
    if (visited.has(venueId)) {
      removeVisitedSource(venueId, 'route')
    } else {
      addVisitedSource(venueId, 'route')
    }
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
            const visitedIds = visited
            const firstUnvisited = r.stops.findIndex((s) => !visitedIds.has(s.venue_id))
            const newIdx = firstUnvisited >= 0 ? firstUnvisited : r.stops.length - 1
            setCurrentStopIdx(newIdx)
            try {
              localStorage.setItem(`zooguide:currentStop:${r.id}`, String(newIdx))
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
            const firstUnvisited = r.stops.findIndex((s) => !visited.has(s.venue_id))
            const newIdx = firstUnvisited >= 0 ? firstUnvisited : r.stops.length - 1
            setCurrentStopIdx(newIdx)
            try {
              localStorage.setItem(`zooguide:currentStop:${r.id}`, String(newIdx))
            } catch {}
          }}
          onRestartQuiz={() => onRestartQuiz?.()}
          onOpenChat={() => onOpenChat?.()}
          onResetProgress={() => {
            setCurrentStopIdx(0)
            try {
              localStorage.setItem(`zooguide:currentStop:${route.id}`, '0')
            } catch {}
            setSubTab('current')
          }}
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