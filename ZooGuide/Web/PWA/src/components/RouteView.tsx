import { useState } from 'react'
import type { NearestResponse, Route, RouteStop, UserPreference } from '../types'
import { api } from '../api/client'
import { ReplanDialog } from './ReplanDialog'
import { PhotoEvalDialog } from './PhotoEvalDialog'
import { loadVisited, saveVisited } from '../lib/storage'

interface Props {
  route: Route
  prefs: UserPreference
  onRouteUpdate: (r: Route) => void
  onReset: () => void
  onChat?: () => void
  onVariants?: () => void
  isFullscreen?: boolean
}

export function RouteView({ route, prefs, onRouteUpdate, onReset, onChat, onVariants, isFullscreen }: Props) {
  const [visited, setVisited] = useState<Set<string>>(loadVisited())
  const [replanOpen, setReplanOpen] = useState(false)
  const [photoOpen, setPhotoOpen] = useState(false)
  const [currentStopIdx, setCurrentStopIdx] = useState(0)
  const [nearest, setNearest] = useState<NearestResponse | null>(null)
  const [locating, setLocating] = useState(false)
  const [locationError, setLocationError] = useState<string | null>(null)

  function toggleVisited(venueId: string) {
    const next = new Set(visited)
    if (next.has(venueId)) next.delete(venueId)
    else next.add(venueId)
    setVisited(next)
    saveVisited(next)
  }

  function applyReplan(updated: Route) {
    onRouteUpdate(updated)
    setReplanOpen(false)
  }

  function pickVenueFromExternal(venueId: string) {
    const idx = route.stops.findIndex((s) => s.venue_id === venueId)
    if (idx >= 0) {
      setCurrentStopIdx(idx)
      // Scroll into view
      setTimeout(() => {
        const el = document.getElementById(`stop-${idx}`)
        el?.scrollIntoView({ behavior: 'smooth', block: 'center' })
      }, 100)
    }
  }

  function elapsedFor(idx: number): number {
    let total = 0
    for (let i = 0; i < idx && i < route.stops.length; i++) {
      total += route.stops[i].visit_minutes + route.stops[i].walk_to_next_minutes
    }
    return total
  }

  function locate() {
    setLocating(true)
    setLocationError(null)
    if (!navigator.geolocation) {
      setLocationError('浏览器不支持定位')
      setLocating(false)
      return
    }
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const r = await api.nearest(pos.coords.latitude, pos.coords.longitude, 3)
          setNearest(r)
          // If top result is a route stop, jump to it
          const top = r.results[0]
          if (top) {
            const idx = route.stops.findIndex((s) => s.venue_id === top.id)
            if (idx >= 0) {
              setCurrentStopIdx(idx)
              setTimeout(() => {
                const el = document.getElementById(`stop-${idx}`)
                el?.scrollIntoView({ behavior: 'smooth', block: 'center' })
              }, 100)
            }
          }
        } catch (e) {
          setLocationError(e instanceof Error ? e.message : '查询失败')
        } finally {
          setLocating(false)
        }
      },
      (err) => {
        setLocationError(`定位失败：${err.message}`)
        setLocating(false)
      },
      { enableHighAccuracy: true, timeout: 10000 },
    )
  }

  return (
    <div>
      <div className="route-summary">
        <div style={{ fontSize: 13, color: 'var(--fg-muted)' }}>
          为你定制的红山路线
          {route.fallback && <span className="route-fallback-badge">规则推荐</span>}
          {!route.fallback && route.llm_used && <span className="route-llm-badge">✨ LLM 生成</span>}
        </div>
        <div className="route-summary-text" style={{ marginTop: 8 }}>
          {route.summary}
        </div>
        <div className="route-summary-stats">
          <div className="route-summary-stat">
            <div className="route-summary-stat-value">{route.stops.length}</div>
            <div className="route-summary-stat-label">个场馆</div>
          </div>
          <div className="route-summary-stat">
            <div className="route-summary-stat-value">{Math.round(route.total_minutes / 60 * 10) / 10}h</div>
            <div className="route-summary-stat-label">总时长</div>
          </div>
          <div className="route-summary-stat">
            <div className="route-summary-stat-value">{route.total_walk_minutes}</div>
            <div className="route-summary-stat-label">步行分钟</div>
          </div>
        </div>
      </div>

      {route.tips && route.tips.length > 0 && (
        <div className="tips-card">
          <h4>💡 给你的小建议</h4>
          <ul>
            {route.tips.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
          </ul>
        </div>
      )}

      {/* Location-based "我在这里" */}
      <div className="card" style={{ background: '#fff' }}>
        <h3 className="card-title">📍 我在哪里？</h3>
        <button
          className="btn btn-primary btn-full"
          onClick={locate}
          disabled={locating}
        >
          {locating ? '定位中…' : '🛰️ 自动定位当前位置'}
        </button>
        {locationError && (
          <div className="error-banner" style={{ marginTop: 10, fontSize: 12 }}>
            {locationError}
          </div>
        )}
        {nearest && (
          <div style={{ marginTop: 12 }}>
            <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 6 }}>
              {nearest.in_park_estimate ? '✅ 看起来在园区内' : '⚠️ 定位可能在园区外'}
              {' · '}坐标 ({nearest.lat.toFixed(4)}, {nearest.lon.toFixed(4)})
            </div>
            {nearest.results.map((r, i) => (
              <button
                key={r.id}
                className="nearest-row"
                onClick={() => pickVenueFromExternal(r.id)}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <div className="nearest-rank">{i + 1}</div>
                  <div style={{ flex: 1, textAlign: 'left' }}>
                    <div className="nearest-name">{r.name}</div>
                    <div className="nearest-meta">
                      {r.area} · {r.animals.slice(0, 2).join('、')}
                    </div>
                  </div>
                  <div className="nearest-dist">
                    {r.distance_m >= 1000
                      ? `${(r.distance_m / 1000).toFixed(1)}km`
                      : `${Math.round(r.distance_m)}m`}
                  </div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="section-title">📍 路线安排</div>
      {route.stops.map((stop, idx) => (
        <div key={`${stop.venue_id}-${idx}`} id={`stop-${idx}`}>
          <StopCard
            stop={stop}
            index={idx}
            isLast={idx === route.stops.length - 1}
            isVisited={visited.has(stop.venue_id)}
            onToggleVisited={() => toggleVisited(stop.venue_id)}
            onMarkCurrent={() => setCurrentStopIdx(idx)}
            isCurrent={idx === currentStopIdx}
          />
        </div>
      ))}

      {route.warnings && route.warnings.length > 0 && (
        <div className="warnings-card">
          <h4>⚠️ 注意事项</h4>
          <ul>
            {route.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </div>
      )}

      <div className={isFullscreen ? 'bottom-bar fullscreen-bar' : 'bottom-bar'}>
        <button className="btn btn-outline" onClick={onReset}>
          🔄 重新规划
        </button>
        <button className="btn btn-ghost" onClick={onVariants}>
          🧭 对比
        </button>
        {isFullscreen && (
          <button className="btn btn-ghost" onClick={onChat}>
            💬 聊天
          </button>
        )}
        <button className="btn btn-primary" onClick={() => setReplanOpen(true)}>
          ✨ 调整
        </button>
      </div>

      {replanOpen && (
        <ReplanDialog
          route={route}
          prefs={prefs}
          currentStopIdx={currentStopIdx}
          elapsedMinutes={elapsedFor(currentStopIdx)}
          onClose={() => setReplanOpen(false)}
          onApplied={applyReplan}
        />
      )}

      {photoOpen && (
        <PhotoEvalDialog onClose={() => setPhotoOpen(false)} onPickVenue={pickVenueFromExternal} />
      )}
    </div>
  )
}

interface StopCardProps {
  stop: RouteStop
  index: number
  isLast: boolean
  isVisited: boolean
  isCurrent: boolean
  onToggleVisited: () => void
  onMarkCurrent: () => void
}

function StopCard({ stop, index, isLast, isVisited, isCurrent, onToggleVisited, onMarkCurrent }: StopCardProps) {
  return (
    <>
      <div className="stop-card" style={isCurrent ? { borderColor: 'var(--primary)', borderWidth: 2 } : undefined}>
        <div className="stop-header">
          <div className="stop-num">{index + 1}</div>
          <div style={{ flex: 1 }}>
            <div className="stop-name">{stop.venue_name}</div>
            <div className="stop-time">
              <strong>{stop.arrive_time}</strong> – <strong>{stop.leave_time}</strong> · 参观 {stop.visit_minutes} 分钟
              {stop.rest_here && <span className="tag" style={{ background: '#d1fae5' }}>🪑 建议休息</span>}
            </div>
          </div>
        </div>
        <div className="stop-narration">{stop.narration}</div>
        {stop.tips && stop.tips.length > 0 && (
          <ul className="stop-tips">
            {stop.tips.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
          </ul>
        )}
        <div className="stop-actions">
          <button
            className={isCurrent ? 'rest' : ''}
            onClick={onMarkCurrent}
          >
            📍 我在这里
          </button>
          <button
            className={isVisited ? 'checked' : ''}
            onClick={onToggleVisited}
          >
            {isVisited ? '✓ 已打卡' : '🦒 打卡'}
          </button>
        </div>
      </div>
      {!isLast && (
        <div className="stop-walk">
          步行 {stop.walk_to_next_minutes} 分钟 →
        </div>
      )}
    </>
  )
}