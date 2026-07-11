import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'

interface Props {
  onPickVenue?: (venueId: string) => void
}

export function NearbyPage({ onPickVenue }: Props) {
  const [venues, setVenues] = useState<Venue[]>([])
  const [meta, setMeta] = useState<any>(null)
  const [nearest, setNearest] = useState<any>(null)
  const [locating, setLocating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
  }, [])

  function locate() {
    setLocating(true)
    setError(null)
    if (!navigator.geolocation) {
      setError('浏览器不支持定位')
      setLocating(false)
      return
    }
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const r = await api.nearest(pos.coords.latitude, pos.coords.longitude, 5)
          setNearest(r)
        } catch (e) {
          setError(e instanceof Error ? e.message : '查询失败')
        } finally {
          setLocating(false)
        }
      },
      (err) => {
        setError(`定位失败：${err.message}`)
        setLocating(false)
      },
      { enableHighAccuracy: true, timeout: 10000 },
    )
  }

  // Group venues by area
  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const area = v.area || '其他'
    if (!byArea[area]) byArea[area] = []
    byArea[area].push(v)
  })

  return (
    <div>
      <div className="card">
        <h3 className="card-title">📍 我在哪？</h3>
        <button className="btn btn-primary btn-full" onClick={locate} disabled={locating}>
          {locating ? '定位中…' : '🛰️ 自动定位当前位置'}
        </button>
        {error && (
          <div className="error-banner" style={{ marginTop: 10, fontSize: 12 }}>
            {error}
          </div>
        )}
        {nearest && (
          <div style={{ marginTop: 12 }}>
            <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 6 }}>
              {nearest.in_park_estimate ? '✅ 看起来在园区内' : '⚠️ 定位可能在园区外'}
              {' · '}坐标 ({nearest.lat.toFixed(4)}, {nearest.lon.toFixed(4)})
            </div>
            {nearest.results.map((r: any, i: number) => (
              <button
                key={r.id}
                className="nearest-row"
                onClick={() => onPickVenue?.(r.id)}
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

      <div className="section-title">📋 全部场馆（{venues.length}）</div>
      {Object.entries(byArea).map(([area, vs]) => (
        <div key={area} style={{ marginBottom: 16 }}>
          <div
            style={{
              fontSize: 12,
              fontWeight: 700,
              color: 'var(--primary-strong)',
              marginBottom: 8,
              padding: '4px 8px',
              background: 'var(--primary-soft)',
              borderRadius: 6,
              display: 'inline-block',
            }}
          >
            {area}（{vs.length} 个）
          </div>
          {vs.map((v) => (
            <div key={v.id} className="venue-card">
              <div className="venue-card-row">
                <div style={{ flex: 1 }}>
                  <div className="venue-card-name">
                    {v.must_see && <span style={{ color: 'var(--accent)', marginRight: 4 }}>⭐</span>}
                    {v.name}
                  </div>
                  <div className="venue-card-meta">
                    {v.animals.slice(0, 3).join('、')}
                    {v.animals.length > 3 && '...'}
                  </div>
                </div>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>
                  {v.recommended_visit_minutes}min
                </div>
              </div>
              {v.tags && v.tags.length > 0 && (
                <div style={{ marginTop: 6 }}>
                  {v.tags
                    .filter((t) => t.length < 8)
                    .slice(0, 3)
                    .map((t, i) => (
                      <span key={i} className="tag">
                        {t}
                      </span>
                    ))}
                </div>
              )}
            </div>
          ))}
        </div>
      ))}
    </div>
  )
}