import { useState } from 'react'
import type { Route, RouteStop } from '../../types'

interface Props {
  route: Route
  prefs: any
  currentStopIdx: number
  onMarkCurrent: (idx: number) => void
  onToggleVisited: (venueId: string) => void
  onOpenStop: (idx: number) => void
}

export function CurrentRouteTab({
  route,
  currentStopIdx,
  onMarkCurrent,
  onToggleVisited,
  onOpenStop,
}: Props) {
  const visited = loadVisited()
  const stops: RouteStop[] = route.stops
  const total = stops.length
  const visitedCount = stops.filter((s: RouteStop) => visited.has(s.venue_id)).length
  const progress = total > 0 ? visitedCount / total : 0

  const currentStop = stops[Math.min(currentStopIdx, total - 1)]
  const nextStop = stops[currentStopIdx + 1]

  return (
    <div className="current-tab">
      {/* Current progress card */}
      <div className="progress-card">
        <div className="progress-header">
          <div className="progress-step">
            第 {currentStopIdx + 1} / {total} 馆
          </div>
          <div className="progress-percent">
            {visitedCount}/{total} 已打卡
          </div>
        </div>

        {/* Progress bar */}
        <div className="progress-bar">
          <div className="progress-bar-fill" style={{ width: `${progress * 100}%` }} />
        </div>

        {/* Current stop */}
        {currentStop && (
          <div className="current-stop-card">
            <div className="cs-header">
              <span className="cs-num">{currentStopIdx + 1}</span>
              <div style={{ flex: 1 }}>
                <div className="cs-name">{currentStop.venue_name}</div>
                <div className="cs-time">
                  🕐 {currentStop.arrive_time} – {currentStop.leave_time} ·{' '}
                  {currentStop.visit_minutes}min
                </div>
              </div>
            </div>
            {currentStop.narration && (
              <div className="cs-narration">{currentStop.narration}</div>
            )}
            <div className="cs-actions">
              <button
                className="cs-btn primary"
                onClick={() => onOpenStop(currentStopIdx)}
              >
                📖 查看讲解
              </button>
              <button
                className={`cs-btn ghost ${visited.has(currentStop.venue_id) ? 'on' : ''}`}
                onClick={() => onToggleVisited(currentStop.venue_id)}
              >
                {visited.has(currentStop.venue_id) ? '✓ 已打卡' : '🦒 打卡'}
              </button>
            </div>
          </div>
        )}

        {/* Next stop preview */}
        {nextStop && (
          <div className="next-stop">
            <div className="ns-label">↓ 下一站 · 步行 {nextStop.walk_to_next_minutes}min</div>
            <div className="ns-name">
              {nextStop.venue_name}
              <span className="ns-time"> {nextStop.arrive_time}</span>
            </div>
          </div>
        )}

        {!nextStop && currentStopIdx === total - 1 && (
          <div className="next-stop finish">
            <div className="ns-label">🎉 最后一馆</div>
            <div className="ns-name">逛完啦！打道回府 🏠</div>
          </div>
        )}
      </div>

      {/* Full route overview */}
      <div className="overview-card">
        <div className="overview-title">📋 路线全览</div>
        <div className="overview-list">
          {stops.map((s, i) => (
            <StopRow
              key={`${s.venue_id}-${i}`}
              stop={s}
              idx={i}
              isVisited={visited.has(s.venue_id)}
              isCurrent={i === currentStopIdx}
              onMarkCurrent={() => onMarkCurrent(i)}
              onToggleVisited={() => onToggleVisited(s.venue_id)}
              onOpen={() => onOpenStop(i)}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function StopRow({
  stop,
  idx,
  isVisited,
  isCurrent,
  onMarkCurrent,
  onToggleVisited,
  onOpen,
}: {
  stop: RouteStop
  idx: number
  isVisited: boolean
  isCurrent: boolean
  onMarkCurrent: () => void
  onToggleVisited: () => void
  onOpen: () => void
}) {
  return (
    <div
      className={`stop-row ${isCurrent ? 'current' : ''} ${isVisited ? 'visited' : ''}`}
      onClick={onOpen}
    >
      <div className="stop-row-num">{idx + 1}</div>
      <div className="stop-row-body">
        <div className="stop-row-name">{stop.venue_name}</div>
        <div className="stop-row-meta">
          🕐 {stop.arrive_time} – {stop.leave_time} · {stop.visit_minutes}min
          {stop.rest_here && <span className="tag" style={{ marginLeft: 4 }}>🪑 歇脚</span>}
        </div>
      </div>
      <div className="stop-row-actions" onClick={(e) => e.stopPropagation()}>
        {isVisited && <span className="stop-row-mark">✓</span>}
        <button
          className="stop-row-btn"
          onClick={onMarkCurrent}
          title={isCurrent ? '当前所在' : '标记为当前'}
        >
          📍
        </button>
      </div>
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