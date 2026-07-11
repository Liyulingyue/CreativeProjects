import { useState } from 'react'
import type { Route, RouteStop } from '../../types'

interface Props {
  route: Route
  prefs: any
  currentStopIdx: number
  onMarkCurrent: (idx: number) => void
  onToggleVisited: (venueId: string) => void
}

interface AreaGroup {
  area: string
  stops: Array<{ stop: RouteStop; idx: number }>
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
  const remainingCount = total - visitedCount

  const currentStop = stops[Math.min(currentStopIdx, total - 1)]
  const nextStop = stops[currentStopIdx + 1]

  function handleVisitedAndAdvance(venueId: string) {
    onToggleVisited(venueId)
    const idx = stops.findIndex((s) => s.venue_id === venueId)
    if (idx === currentStopIdx && idx < total - 1) {
      setTimeout(() => onMarkCurrent(idx + 1), 200)
    }
  }

  // Group by area
  const areaMap: Record<string, AreaGroup> = {}
  stops.forEach((s, i) => {
    const area = (s as any).area || '其他'
    if (!areaMap[area]) areaMap[area] = { area, stops: [] }
    areaMap[area].stops.push({ stop: s, idx: i })
  })
  const areaGroups = Object.values(areaMap)

  // Overview collapse state (default expanded)
  const [overviewOpen, setOverviewOpen] = useState(true)

  return (
    <div className="current-tab">
      {/* ===== Section 1: Current progress + Next ===== */}
      <div className="progress-card">
        <div className="progress-header">
          <div className="progress-step">
            第 {currentStopIdx + 1} / {total} 馆
          </div>
          <div className="progress-percent">
            {visitedCount}/{total} 已游览
          </div>
        </div>

        <div className="progress-bar">
          <div
            className="progress-bar-fill"
            style={{ width: `${progress * 100}%` }}
          />
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
                className={`cs-btn ${visited.has(currentStop.venue_id) ? 'on' : 'ghost'}`}
                onClick={() => handleVisitedAndAdvance(currentStop.venue_id)}
              >
                {visited.has(currentStop.venue_id)
                  ? '✓ 已游览'
                  : '🦒 标记为已游览'}
              </button>
              {currentStop.rest_here && (
                <span
                  style={{
                    fontSize: 11,
                    color: 'var(--fg-muted)',
                    display: 'flex',
                    alignItems: 'center',
                  }}
                >
                  🪑 歇脚点
                </span>
              )}
            </div>
          </div>
        )}

        {/* Next stop */}
        {nextStop && (
          <div className="next-stop">
            <div className="ns-label">
              ↓ 下一站 · 步行 {nextStop.walk_to_next_minutes}min
            </div>
            <div className="ns-name">
              {nextStop.venue_name}
              <span className="ns-time"> {nextStop.arrive_time}</span>
            </div>
          </div>
        )}

        {!nextStop && currentStopIdx === total - 1 && (
          <div className="next-stop finish">
            <div className="ns-label">🎉 已游览全部</div>
            <div className="ns-name">完成今日探索 🎊</div>
          </div>
        )}
      </div>

      {/* ===== Section 2: 总览 (overview, collapsible) ===== */}
      <div className="route-overview-card">
        <div className="roc-header" onClick={() => setOverviewOpen(!overviewOpen)}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, minWidth: 0 }}>
            <span style={{ fontSize: 20 }}>🧭</span>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div className="roc-label">总览</div>
              <div className="roc-summary">{route.summary?.slice(0, 60) || '今天逛这些'}</div>
            </div>
          </div>
          <div className="roc-quickstats">
            <span><strong>{total}</strong> 馆</span>
            <span><strong>{Math.round(route.total_minutes / 60 * 10) / 10}</strong> h</span>
          </div>
          <div className="roc-toggle">{overviewOpen ? '−' : '+'}</div>
        </div>

        {overviewOpen && (
          <div className="roc-body">
            <div className="roc-stops-mini">
              {stops.map((s, i) => (
                <div
                  key={`${s.venue_id}-${i}`}
                  className={`roc-mini-stop ${visited.has(s.venue_id) ? 'visited' : ''} ${
                    i === currentStopIdx ? 'current' : ''
                  }`}
                  onClick={(e) => {
                    e.stopPropagation()
                    onMarkCurrent(i)
                  }}
                  title={`跳到第 ${i + 1} 馆`}
                >
                  <span className="roc-mini-num">{i + 1}</span>
                  <span className="roc-mini-name">{s.venue_name}</span>
                  <span className="roc-mini-time">{s.arrive_time?.slice(0, 5)}</span>
                  {visited.has(s.venue_id) && (
                    <span className="roc-mini-mark visited">✓</span>
                  )}
                  {i === currentStopIdx && (
                    <span className="roc-mini-mark current">📍</span>
                  )}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* ===== Section 3: 各区域单览 ===== */}
      <div className="area-sections">
        <div
          style={{
            fontSize: 12,
            fontWeight: 700,
            color: 'var(--primary-strong)',
            marginBottom: 10,
            padding: '4px 8px',
            background: 'var(--primary-soft)',
            borderRadius: 8,
            display: 'inline-block',
          }}
        >
          🗺️ 按区域查看
        </div>
        {areaGroups.map((group) => (
          <div key={group.area} className="area-section">
            <div className="area-section-header">
              <span className="area-section-icon">📍</span>
              <span className="area-section-name">{group.area}</span>
              <span className="area-section-count">
                {group.stops.length} 馆
              </span>
            </div>
            {group.stops.map(({ stop, idx }) => {
              const isVisited = visited.has(stop.venue_id)
              const isCurrent = idx === currentStopIdx
              return (
                <AreaStopCard
                  key={`${stop.venue_id}-${idx}`}
                  stop={stop}
                  idx={idx}
                  isVisited={isVisited}
                  isCurrent={isCurrent}
                  onMarkCurrent={() => onMarkCurrent(idx)}
                  onToggleVisited={() => handleVisitedAndAdvance(stop.venue_id)}
                />
              )
            })}
          </div>
        ))}
      </div>

      {remainingCount === 0 && (
        <div className="finish-banner">
          🎉 全部完成！期待下一次见面
        </div>
      )}
    </div>
  )
}

function AreaStopCard({
  stop,
  idx,
  isVisited,
  isCurrent,
  onMarkCurrent,
  onToggleVisited,
}: {
  stop: RouteStop
  idx: number
  isVisited: boolean
  isCurrent: boolean
  onMarkCurrent: () => void
  onToggleVisited: () => void
}) {
  const [expanded, setExpanded] = useState(isCurrent)

  return (
    <div
      className={`area-stop-card ${isCurrent ? 'current' : ''} ${isVisited ? 'visited' : ''}`}
      onClick={() => setExpanded(!expanded)}
    >
      <div className="asc-header">
        <div className="asc-num">{idx + 1}</div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div className="asc-name">{stop.venue_name}</div>
          <div className="asc-meta">
            🕐 {stop.arrive_time?.slice(0, 5)} – {stop.leave_time?.slice(0, 5)} ·{' '}
            {stop.visit_minutes}min
            {stop.rest_here && <span style={{ marginLeft: 4 }}>🪑</span>}
          </div>
        </div>
        <div className="asc-tags">
          {isCurrent && <span className="asc-tag current">📍 当前</span>}
          {isVisited && <span className="asc-tag visited">✓</span>}
        </div>
        <div className="asc-toggle">{expanded ? '−' : '+'}</div>
      </div>

      {expanded && (
        <div className="asc-body" onClick={(e) => e.stopPropagation()}>
          {stop.narration && (
            <div className="asc-narration">{stop.narration}</div>
          )}
          {stop.tips && stop.tips.length > 0 && (
            <div className="asc-tips">
              {stop.tips.map((t, i) => (
                <div key={i} className="asc-tip">
                  💡 {t}
                </div>
              ))}
            </div>
          )}
          <div className="asc-actions">
            <button
              className={`asc-btn ${isCurrent ? 'on' : 'ghost'}`}
              onClick={onMarkCurrent}
            >
              📍 我在这里
            </button>
            <button
              className={`asc-btn primary ${isVisited ? 'on' : ''}`}
              onClick={onToggleVisited}
            >
              {isVisited ? '✓ 已游览' : '🦒 已游览'}
            </button>
          </div>
        </div>
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