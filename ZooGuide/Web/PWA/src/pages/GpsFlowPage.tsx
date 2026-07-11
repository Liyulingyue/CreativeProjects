import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import type { Venue } from '../types'
import { api } from '../api/client'
import { loadActivityVisited, saveActivityVisited } from '../lib/storage'

interface CheckinResult {
  distance_m: number
  success: boolean
  message: string
}

const ACTIVITY = 'gps'

export function GpsFlowPage() {
  const navigate = useNavigate()
  const [venues, setVenues] = useState<Venue[]>([])
  const [visited, setVisited] = useState<Set<string>>(loadActivityVisited(ACTIVITY))
  const [selectedVenue, setSelectedVenue] = useState<Venue | null>(null)
  const [locating, setLocating] = useState(false)
  const [result, setResult] = useState<CheckinResult | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
  }, [])

  function handleVenueClick(v: Venue) {
    setSelectedVenue(v)
    setResult(null)
    setError(null)
    setLocating(true)

    if (!navigator.geolocation) {
      setLocating(false)
      setError('浏览器不支持定位')
      return
    }

    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const r = await api.nearest(pos.coords.latitude, pos.coords.longitude, 20)
          const matched = r.results?.find((n: any) => n.id === v.id)
          const dist = matched?.distance_m ?? Infinity
          const inPark = r.in_park_estimate

          fetch('/api/gps-checkin', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              lat: pos.coords.latitude,
              lon: pos.coords.longitude,
              in_park: inPark,
              nearest_venue_id: v.id,
              nearest_venue_name: v.name,
            }),
          }).catch(() => {})

          const success = dist <= 200
          if (success) {
            const next = new Set(loadActivityVisited(ACTIVITY))
            next.add(v.id)
            saveActivityVisited(ACTIVITY, next)
            setVisited(next)
          }

          setResult({
            distance_m: dist,
            success,
            message: success
              ? `距离 ${dist < 1000 ? Math.round(dist) + 'm' : (dist / 1000).toFixed(1) + 'km'}，打卡成功！`
              : `距离 ${dist < 1000 ? Math.round(dist) + 'm' : (dist / 1000).toFixed(1) + 'km'}，太远了（需 200m 内）`,
          })
        } catch (e) {
          setError(e instanceof Error ? e.message : '查询失败')
        }
        setLocating(false)
      },
      (err) => {
        setLocating(false)
        setError(`定位失败：${err.message}`)
      },
      { enableHighAccuracy: true, timeout: 10000 },
    )
  }

  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const a = v.area || '场馆'
    if (!byArea[a]) byArea[a] = []
    byArea[a].push(v)
  })

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/activity')}>←</button>
        <div className="flow-title">📍 GPS 打卡</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
          <MiniStat value={visited.size} label="已打卡" highlight />
          <MiniStat value={venues.length} label="总场馆" />
        </div>

        {Object.entries(byArea).map(([area, list]) => (
        <div key={area} className="venue-list-section">
          <div className="venue-list-header">
            <span>📍</span>
            <span>{area}</span>
            <span className="venue-list-count">
              {list.filter((v) => visited.has(v.id)).length}/{list.length}
            </span>
          </div>
          {list.map((v) => {
            const isVisited = visited.has(v.id)
            return (
              <button
                key={v.id}
                className={`venue-list-item ${isVisited ? 'visited' : ''}`}
                onClick={() => handleVenueClick(v)}
              >
                <div className="venue-list-emoji">{venueEmoji(v.id)}</div>
                <div className="venue-list-body">
                  <div className="venue-list-name">{v.name}</div>
                  <div className="venue-list-meta">
                    {v.animals.slice(0, 2).join(' · ')}
                  </div>
                </div>
                <div className="venue-list-status">
                  {isVisited ? (
                    <span className="venue-list-checked">✓ 已打卡</span>
                  ) : (
                    <span className="venue-list-tap">定位打卡 ›</span>
                  )}
                </div>
              </button>
            )
          })}
        </div>
        ))}
      </div>

      {selectedVenue && (
        <div className="modal-mask" onClick={() => { setSelectedVenue(null); setResult(null); setError(null) }}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{venueEmoji(selectedVenue.id)} {selectedVenue.name}</h3>

            {locating && (
              <div style={{ textAlign: 'center', padding: '20px 0' }}>
                <div className="spinner" />
                <div style={{ marginTop: 12, color: 'var(--fg-muted)', fontSize: 13 }}>正在获取位置…</div>
              </div>
            )}

            {error && (
              <div style={{ textAlign: 'center', padding: '12px 0' }}>
                <div style={{ fontSize: 28, marginBottom: 8 }}>⚠️</div>
                <div style={{ fontSize: 13, color: 'var(--fg-muted)' }}>{error}</div>
                <button className="btn btn-ghost" style={{ marginTop: 10 }} onClick={() => handleVenueClick(selectedVenue)}>
                  重试
                </button>
              </div>
            )}

            {result && (
              <div style={{ textAlign: 'center', padding: '8px 0' }}>
                <div style={{ fontSize: 28, marginBottom: 8 }}>{result.success ? '✓' : '📍'}</div>
                <div style={{
                  fontSize: 14,
                  fontWeight: 700,
                  color: result.success ? '#10b981' : '#f59e0b',
                }}>
                  {result.success ? '打卡成功' : '距离太远'}
                </div>
                <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginTop: 4 }}>
                  {result.message}
                </div>
              </div>
            )}

            {!locating && !error && !result && (
              <div style={{ textAlign: 'center', padding: '12px 0', color: 'var(--fg-muted)', fontSize: 13 }}>
                点击即获取位置并打卡
              </div>
            )}

            <div className="modal-actions" style={{ marginTop: 12 }}>
              <button className="btn btn-ghost btn-full" onClick={() => { setSelectedVenue(null); setResult(null); setError(null) }}>
                关闭
              </button>
              {!locating && result && !result.success && (
                <button className="btn btn-primary btn-full" onClick={() => handleVenueClick(selectedVenue)}>
                  🔄 重试
                </button>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function MiniStat({ value, label, highlight }: { value: number; label: string; highlight?: boolean }) {
  return (
    <div
      style={{
        flex: 1,
        background: highlight ? 'var(--primary)' : 'var(--bg-elev)',
        color: highlight ? 'white' : 'var(--fg)',
        border: '1px solid ' + (highlight ? 'var(--primary)' : 'var(--border)'),
        borderRadius: 8,
        padding: '6px 4px',
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 16, fontWeight: 700, lineHeight: 1.2 }}>{value}</div>
      <div style={{ fontSize: 10, opacity: 0.8 }}>{label}</div>
    </div>
  )
}

function venueEmoji(venueId: string): string {
  const map: Record<string, string> = {
    panda: '🐼', koala: '🐨', gorilla: '🦍', tiger: '🐯',
    giraffe: '🦒', meerkat: '🦝', red_panda: '🐾', tangjiahe: '🏔️',
    hornbill: '🦜', crane: '🦢', monkey_mountain: '🐒', bear: '🐻',
  }
  return map[venueId] || '📍'
}
