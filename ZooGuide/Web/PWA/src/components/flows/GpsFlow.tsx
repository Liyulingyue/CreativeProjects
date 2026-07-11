import { useEffect, useState } from 'react'
import { api } from '../../api/client'
import { useVisitedVenues } from '../../hooks/useVisitedVenues'
import { saveVisited } from '../../lib/storage'

interface Props {
  onClose: () => void
  onOpenPlan: () => void
}

interface NearestVenue {
  id: string
  name: string
  area: string
  animals: string[]
  distance_m: number
}

export function GpsFlow({ onClose, onOpenPlan }: Props) {
  const { visited } = useVisitedVenues()
  const [state, setState] = useState<{
    locating?: boolean
    error?: string | null
    nearest?: any
  }>({})

  function locate() {
    setState({ locating: true, error: null })
    if (!navigator.geolocation) {
      setState({ locating: false, error: '浏览器不支持定位' })
      return
    }
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const r = await api.nearest(pos.coords.latitude, pos.coords.longitude, 8)
          setState({ nearest: r, locating: false })
          // POST GPS checkin (triggers achievement eval)
          fetch('/api/gps-checkin', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              lat: pos.coords.latitude,
              lon: pos.coords.longitude,
              in_park: r.in_park_estimate,
              nearest_venue_id: r.results[0]?.id,
              nearest_venue_name: r.results[0]?.name,
            }),
          }).catch(() => {})
        } catch (e) {
          setState({ locating: false, error: e instanceof Error ? e.message : '查询失败' })
        }
      },
      (err) => {
        setState({ locating: false, error: `定位失败：${err.message}` })
      },
      { enableHighAccuracy: true, timeout: 10000 },
    )
  }

  function quickCheckin(venueId: string) {
    const next = new Set(visited)
    if (next.has(venueId)) next.delete(venueId)
    else next.add(venueId)
    saveVisited(next)
    fetch('/api/checkin', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ venue_id: venueId }),
    }).catch(() => {})
  }

  // Auto-locate on mount
  useEffect(() => {
    locate()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={onClose}>
          ←
        </button>
        <div className="flow-title">📍 GPS 打卡</div>
        <button
          className="flow-back"
          onClick={locate}
          style={{ background: 'rgba(255,255,255,0.18)' }}
          title="重新定位"
        >
          {state.locating ? '⏳' : '🔄'}
        </button>
      </header>

      <div className="flow-body">
        {/* 状态卡 */}
        <div
          style={{
            background: 'linear-gradient(135deg, #0891b2, #0e7490)',
            color: 'white',
            borderRadius: 14,
            padding: 20,
            textAlign: 'center',
            marginBottom: 14,
          }}
        >
          {state.locating && (
            <>
              <div style={{ fontSize: 40, marginBottom: 8 }}>🛰️</div>
              <div style={{ fontSize: 16, fontWeight: 700 }}>定位中…</div>
              <div style={{ fontSize: 12, opacity: 0.85, marginTop: 4 }}>
                需要位置权限
              </div>
            </>
          )}
          {!state.locating && state.error && (
            <>
              <div style={{ fontSize: 40, marginBottom: 8 }}>⚠️</div>
              <div style={{ fontSize: 14, fontWeight: 600 }}>定位失败</div>
              <div style={{ fontSize: 12, opacity: 0.85, marginTop: 6 }}>{state.error}</div>
              <button
                className="btn"
                style={{
                  marginTop: 12,
                  background: 'white',
                  color: '#0e7490',
                }}
                onClick={locate}
              >
                🔄 重试
              </button>
            </>
          )}
          {!state.locating && state.nearest && (
            <>
              <div style={{ fontSize: 32, marginBottom: 6 }}>📍</div>
              <div style={{ fontSize: 18, fontWeight: 700 }}>
                {state.nearest.results[0]?.name}
              </div>
              <div style={{ fontSize: 12, opacity: 0.85, marginTop: 4 }}>
                {state.nearest.in_park_estimate ? '✅ 在园区内' : '⚠️ 可能在园区外'}
                {' · '}
                {state.nearest.results[0]?.distance_m >= 1000
                  ? `${(state.nearest.results[0].distance_m / 1000).toFixed(2)}km`
                  : `${Math.round(state.nearest.results[0]?.distance_m || 0)}m`}
              </div>
            </>
          )}
        </div>

        {/* 附近馆列表 */}
        {state.nearest && (
          <>
            <div className="activity-section-title">
              <span>附近 8 个馆</span>
            </div>
            {state.nearest.results.map((r: NearestVenue, i: number) => (
              <button
                key={r.id}
                className="nearest-row"
                onClick={() => quickCheckin(r.id)}
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
                      ? `${(r.distance_m / 1000).toFixed(2)}km`
                      : `${Math.round(r.distance_m)}m`}
                  </div>
                  <div
                    className={`activity-checkin-mark ${
                      visited.has(r.id) ? 'on' : ''
                    }`}
                    style={{ position: 'static' }}
                  >
                    {visited.has(r.id) ? '✓' : '+'}
                  </div>
                </div>
              </button>
            ))}

            <button
              className="btn btn-primary btn-full"
              style={{ marginTop: 14 }}
              onClick={onOpenPlan}
            >
              🧭 规划包含 {state.nearest.results[0]?.name} 的路线
            </button>
          </>
        )}
      </div>
    </div>
  )
}