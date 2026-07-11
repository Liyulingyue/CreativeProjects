import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoEvalDialog } from '../components/PhotoEvalDialog'

export function ActivityPage() {
  const [venues, setVenues] = useState<Venue[]>([])
  const [nearest, setNearest] = useState<any>(null)
  const [locating, setLocating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [photoOpen, setPhotoOpen] = useState(false)
  const [checkedIn, setCheckedIn] = useState<Set<string>>(() => {
    try {
      const raw = localStorage.getItem('zooguide:visited:v1')
      return new Set(raw ? JSON.parse(raw) : [])
    } catch {
      return new Set()
    }
  })

  useEffect(() => {
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

  function quickCheckin(venueId: string) {
    const next = new Set(checkedIn)
    if (next.has(venueId)) next.delete(venueId)
    else next.add(venueId)
    setCheckedIn(next)
    localStorage.setItem('zooguide:visited:v1', JSON.stringify([...next]))
    // Optional: also POST to backend if logged in
    fetch('/api/checkin', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ venue_id: venueId }),
    }).catch(() => {})
  }

  const mustSee = venues.filter((v) => v.must_see)

  return (
    <div>
      {/* 3 large action cards */}
      <div style={{ display: 'grid', gap: 10 }}>
        <button
          className="activity-card"
          onClick={() => setPhotoOpen(true)}
          style={{ background: 'linear-gradient(135deg, #fde68a, #fff7ed)' }}
        >
          <div className="activity-icon">📸</div>
          <div className="activity-body">
            <div className="activity-title">出片彩蛋</div>
            <div className="activity-desc">拍/选一张动物照，Agent 给你打分 + 出徽章</div>
          </div>
          <div className="activity-arrow">›</div>
        </button>

        <button className="activity-card" onClick={locate} disabled={locating}>
          <div className="activity-icon">🛰️</div>
          <div className="activity-body">
            <div className="activity-title">{locating ? '定位中…' : '自动定位'}</div>
            <div className="activity-desc">看我当前在园区哪里</div>
          </div>
          <div className="activity-arrow">›</div>
        </button>
      </div>

      {/* GPS result */}
      {error && (
        <div className="error-banner" style={{ marginTop: 10, fontSize: 12 }}>
          {error}
        </div>
      )}
      {nearest && (
        <div className="card" style={{ marginTop: 10 }}>
          <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 8 }}>
            {nearest.in_park_estimate ? '✅ 看起来在园区内' : '⚠️ 定位可能在园区外'} · 距{' '}
            {nearest.results[0]?.name} {nearest.results[0]?.distance_m >= 1000
              ? `${(nearest.results[0].distance_m / 1000).toFixed(1)}km`
              : `${Math.round(nearest.results[0]?.distance_m || 0)}m`}
          </div>
          {nearest.results.slice(0, 3).map((r: any, i: number) => (
            <div key={r.id} className="nearest-row">
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
                <button
                  className={`checkin-btn ${checkedIn.has(r.id) ? 'on' : ''}`}
                  onClick={() => quickCheckin(r.id)}
                  style={{ marginLeft: 8 }}
                >
                  {checkedIn.has(r.id) ? '✓' : '🦒'}
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Must-see quick checkin */}
      <div className="section-title">⭐ 必看馆（一键打卡）</div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
        {mustSee.slice(0, 12).map((v) => (
          <button
            key={v.id}
            className={`checkin-tile ${checkedIn.has(v.id) ? 'on' : ''}`}
            onClick={() => quickCheckin(v.id)}
          >
            <div className="checkin-tile-name">{v.name}</div>
            <div className="checkin-tile-meta">
              {v.animals.slice(0, 2).join('、')}
            </div>
            <div className="checkin-tile-mark">{checkedIn.has(v.id) ? '✓' : '+'}</div>
          </button>
        ))}
      </div>

      {/* Tip */}
      <div
        className="card"
        style={{ marginTop: 14, fontSize: 12, color: 'var(--fg-muted)' }}
      >
        💡 打卡会自动同步到「我的」页面。已打卡 <strong>{checkedIn.size}</strong> /{' '}
        {venues.length} 个馆。
      </div>

      {photoOpen && <PhotoEvalDialog onClose={() => setPhotoOpen(false)} />}
    </div>
  )
}