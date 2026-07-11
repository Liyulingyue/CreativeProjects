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
}: Props) {
  const visited = loadVisited()
  const stops: RouteStop[] = route.stops
  const total = stops.length
  const visitedCount = stops.filter((s) => visited.has(s.venue_id)).length
  const progress = total > 0 ? visitedCount / total : 0

  const currentStop = stops[Math.min(currentStopIdx, total - 1)]
  const nextStop = stops[currentStopIdx + 1]
  const remainingCount = total - visitedCount

  function handleVisitedAndAdvance(venueId: string) {
    onToggleVisited(venueId)
    // Auto-advance: if this was the current stop and not last, move to next
    const idx = stops.findIndex((s) => s.venue_id === venueId)
    if (idx === currentStopIdx && idx < total - 1) {
      setTimeout(() => onMarkCurrent(idx + 1), 200)
    }
  }

  return (
    <div className="current-tab">
      {/* ===== Top: Route Overview CARD (variant style) ===== */}
      <div className="route-overview-card">
        <div className="roc-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1 }}>
            <span style={{ fontSize: 24 }}>🧭</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>当前路线</div>
              <div
                style={{
                  fontSize: 14,
                  fontWeight: 700,
                  color: 'var(--primary-strong)',
                  lineHeight: 1.3,
                }}
              >
                {route.summary?.slice(0, 60) || '今天逛这些'}
              </div>
            </div>
          </div>
          <div className="roc-stats">
            <div className="roc-stat">
              <div className="roc-stat-num">{total}</div>
              <div className="roc-stat-label">馆</div>
            </div>
            <div className="roc-stat">
              <div className="roc-stat-num">
                {Math.round(route.total_minutes / 60 * 10) / 10}
              </div>
              <div className="roc-stat-label">h</div>
            </div>
          </div>
        </div>

        {/* Progress */}
        <div className="roc-progress">
          <div className="roc-progress-bar" style={{ width: `${progress * 100}%` }} />
          <div className="roc-progress-label">
            {visitedCount}/{total} 已游览
            {remainingCount > 0 && ` · 还剩 ${remainingCount} 馆`}
          </div>
        </div>

        {/* Mini stops preview - inline list */}
        <div className="roc-stops-mini">
          {stops.map((s, i) => (
            <div
              key={`${s.venue_id}-${i}`}
              className={`roc-mini-stop ${visited.has(s.venue_id) ? 'visited' : ''} ${
                i === currentStopIdx ? 'current' : ''
              }`}
              onClick={() => onMarkCurrent(i)}
              title={`跳到第 ${i + 1} 馆`}
            >
              <span className="roc-mini-num">{i + 1}</span>
              <span className="roc-mini-name">{s.venue_name}</span>
              {visited.has(s.venue_id) && <span className="roc-mini-mark">✓</span>}
              {i === currentStopIdx && <span className="roc-mini-mark here">📍</span>}
            </div>
          ))}
        </div>
      </div>

      {/* ===== Specific venues (detailed cards) ===== */}
      <div className="stops-detail">
        {stops.map((s, i) => (
          <StopDetailCard
            key={`${s.venue_id}-${i}`}
            stop={s}
            idx={i}
            isVisited={visited.has(s.venue_id)}
            isCurrent={i === currentStopIdx}
            isNext={i === currentStopIdx + 1}
            onMarkCurrent={() => onMarkCurrent(i)}
            onToggleVisited={() => handleVisitedAndAdvance(s.venue_id)}
          />
        ))}
      </div>

      {currentStop && remainingCount > 0 && (
        <div className="next-stop-banner">
          <div className="nsb-label">↓ 下一站 · 步行 {nextStop?.walk_to_next_minutes || 0}min</div>
          <div className="nsb-name">{nextStop?.venue_name}</div>
        </div>
      )}

      {currentStop && remainingCount === 0 && (
        <div className="next-stop-banner finish">
          <div className="nsb-label">🎉 已游览全部</div>
          <div className="nsb-name">完成今日探索 🎊</div>
        </div>
      )}
    </div>
  )
}

function StopDetailCard({
  stop,
  idx,
  isVisited,
  isCurrent,
  isNext,
  onMarkCurrent,
  onToggleVisited,
}: {
  stop: RouteStop
  idx: number
  isVisited: boolean
  isCurrent: boolean
  isNext: boolean
  onMarkCurrent: () => void
  onToggleVisited: () => void
}) {
  const [expanded, setExpanded] = useState(isCurrent)

  return (
    <div
      className={`stop-detail-card ${
        isCurrent ? 'current' : isNext ? 'next' : isVisited ? 'visited' : ''
      }`}
      onClick={() => setExpanded(!expanded)}
    >
      <div className="sdc-header">
        <div className="sdc-num">{idx + 1}</div>
        <div className="sdc-body">
          <div className="sdc-title">
            {stop.venue_name}
            {isCurrent && <span className="sdc-tag current">📍 当前</span>}
            {isNext && <span className="sdc-tag next">↓ 下一站</span>}
            {isVisited && <span className="sdc-tag visited">✓ 已游览</span>}
            {stop.rest_here && <span className="sdc-tag rest">🪑 歇脚</span>}
          </div>
          <div className="sdc-meta">
            🕐 {stop.arrive_time} – {stop.leave_time} ·{' '}
            {stop.visit_minutes}min
            {stop.walk_to_next_minutes > 0 &&
              ` · 步行 ${stop.walk_to_next_minutes}min`}
          </div>
        </div>
        <div className="sdc-toggle">{expanded ? '−' : '+'}</div>
      </div>

      {expanded && (
        <>
          {stop.narration && (
            <div className="sdc-narration">{stop.narration}</div>
          )}

          {stop.tips && stop.tips.length > 0 && (
            <div className="sdc-tips">
              {stop.tips.map((t, i) => (
                <div key={i} className="sdc-tip">💡 {t}</div>
              ))}
            </div>
          )}

          <div className="sdc-actions" onClick={(e) => e.stopPropagation()}>
            <button
              className={`sdc-btn ${isCurrent ? 'on' : 'ghost'}`}
              onClick={onMarkCurrent}
              title="标记为当前所在地"
            >
              📍 我在这里
            </button>
            <button
              className={`sdc-btn visited-btn ${isVisited ? 'on' : 'primary'}`}
              onClick={onToggleVisited}
            >
              {isVisited ? '✓ 已游览' : '🦒 标记为已游览'}
            </button>
          </div>
        </>
      )}
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