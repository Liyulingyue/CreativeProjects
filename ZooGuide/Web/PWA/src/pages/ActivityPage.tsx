import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoEvalDialog } from '../components/PhotoEvalDialog'
import { useVisitedVenues } from '../hooks/useVisitedVenues'
import { saveVisited, loadPhotoLog, type PhotoLogEntry } from '../lib/storage'

export function ActivityPage() {
  const [venues, setVenues] = useState<Venue[]>([])
  const [photoOpen, setPhotoOpen] = useState(false)
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [gpsState, setGpsState] = useState<{
    nearest?: any
    locating?: boolean
    error?: string | null
  }>({})
  const [achievements, setAchievements] = useState<{ new: string[]; lastEval: number }>({
    new: [],
    lastEval: 0,
  })
  const { visited: checkedIn } = useVisitedVenues()

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    function onPhotoLogChanged() {
      setPhotoLog(loadPhotoLog())
    }
    window.addEventListener('zooguide:photoLogChanged', onPhotoLogChanged)
    return () => window.removeEventListener('zooguide:photoLogChanged', onPhotoLogChanged)
  }, [])

  function quickCheckin(venueId: string) {
    const next = new Set(checkedIn)
    if (next.has(venueId)) next.delete(venueId)
    else next.add(venueId)
    saveVisited(next)
    // POST to backend too (may trigger achievement)
    fetch('/api/checkin', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ venue_id: venueId }),
    })
      .then((r) => r.json())
      .then((d) => {
        if (d.new_achievements && d.new_achievements.length > 0) {
          setAchievements({ new: d.new_achievements, lastEval: Date.now() })
        }
      })
      .catch(() => {})
  }

  function locate() {
    setGpsState({ locating: true, error: null })
    if (!navigator.geolocation) {
      setGpsState({ locating: false, error: '浏览器不支持定位' })
      return
    }
    navigator.geolocation.getCurrentPosition(
      async (pos) => {
        try {
          const r = await api.nearest(pos.coords.latitude, pos.coords.longitude, 5)
          setGpsState({ nearest: r, locating: false })
          // POST GPS checkin (may trigger achievement)
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
          })
            .then((resp) => resp.json())
            .then((d) => {
              if (d.new_achievements && d.new_achievements.length > 0) {
                setAchievements({ new: d.new_achievements, lastEval: Date.now() })
              }
            })
            .catch(() => {})
        } catch (e) {
          setGpsState({ locating: false, error: e instanceof Error ? e.message : '查询失败' })
        }
      },
      (err) => {
        setGpsState({ locating: false, error: `定位失败：${err.message}` })
      },
      { enableHighAccuracy: true, timeout: 10000 },
    )
  }

  const todayCount = photoLog.filter((p) => {
    const d = new Date(p.ts)
    const now = new Date()
    return d.toDateString() === now.toDateString()
  }).length
  const maxVibe = photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0
  const unlockedBadges = new Set(photoLog.map((p) => badgeFromVenue(p.matched_venue_id)))

  return (
    <div>
      {achievements.new.length > 0 && (
        <AchievementToast
          ids={achievements.new}
          onClose={() => setAchievements({ new: [], lastEval: 0 })}
        />
      )}

      {/* Card 1: 拍照打卡 */}
      <div
        className="activity-card-main"
        onClick={() => setPhotoOpen(true)}
        style={{ background: 'linear-gradient(135deg, #f59e0b, #d97706)' }}
      >
        <div className="acm-icon">📷</div>
        <div className="acm-body">
          <div className="acm-title">拍照打卡</div>
          <div className="acm-sub">
            拍动物照片 · AI 识别 · 自动记录
            <br />
            今日 {todayCount} 张 · 总 {photoLog.length} 张
          </div>
        </div>
        <div className="acm-arrow">📷</div>
      </div>

      {/* Card 2: 出片评分 */}
      <div className="activity-card-main" style={{ background: 'linear-gradient(135deg, #8b5cf6, #7c3aed)' }}>
        <div className="acm-icon">🌟</div>
        <div className="acm-body">
          <div className="acm-title">出片评分</div>
          <div className="acm-sub">
            已收录 {photoLog.length} 张
            <br />
            最高分 <strong>{maxVibe}</strong>
            {unlockedBadges.size > 0 && ` · ${unlockedBadges.size} 徽章`}
          </div>
        </div>
        <div className="acm-arrow">🏅</div>
      </div>

      {/* Card 3: GPS 打卡 */}
      <div
        className="activity-card-main"
        onClick={locate}
        style={{ background: 'linear-gradient(135deg, #0891b2, #0e7490)' }}
      >
        <div className="acm-icon">📍</div>
        <div className="acm-body">
          <div className="acm-title">GPS 打卡</div>
          <div className="acm-sub">
            {gpsState.locating
              ? '🛰️ 定位中…'
              : gpsState.nearest
              ? `在 ${gpsState.nearest.results[0]?.name} 附近`
              : '看看我在园区哪里'}
            <br />
            {gpsState.nearest && `${gpsState.nearest.results.length} 馆可打卡`}
          </div>
        </div>
        <div className="acm-arrow">🛰️</div>
      </div>

      {/* GPS result / error */}
      {gpsState.error && (
        <div className="error-banner" style={{ marginTop: 10, fontSize: 12 }}>
          {gpsState.error}
        </div>
      )}
      {gpsState.nearest && (
        <div className="gps-results">
          {gpsState.nearest.results.slice(0, 3).map((r: any, i: number) => (
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
                    ? `${(r.distance_m / 1000).toFixed(1)}km`
                    : `${Math.round(r.distance_m)}m`}
                </div>
                <div
                  className={`activity-checkin-mark ${
                    checkedIn.has(r.id) ? 'on' : ''
                  }`}
                  style={{ position: 'static' }}
                >
                  {checkedIn.has(r.id) ? '✓' : '+'}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}

      {/* 备选：必看馆快速打卡 */}
      <details className="activity-alt">
        <summary>不拍照？21 个必看馆一键打卡</summary>
        <div className="activity-checkin-grid">
          {venues
            .filter((v) => v.must_see)
            .map((v) => (
              <button
                key={v.id}
                className={`activity-checkin-tile ${checkedIn.has(v.id) ? 'on' : ''}`}
                onClick={() => quickCheckin(v.id)}
              >
                <div className="activity-checkin-name">{v.name}</div>
                <div
                  className={`activity-checkin-mark ${
                    checkedIn.has(v.id) ? 'on' : ''
                  }`}
                >
                  {checkedIn.has(v.id) ? '✓' : '+'}
                </div>
              </button>
            ))}
        </div>
      </details>

      <div
        style={{
          fontSize: 11,
          color: 'var(--fg-muted)',
          textAlign: 'center',
          marginTop: 12,
        }}
      >
        💡 拍照 / GPS / 打卡 都可能解锁活动成就（Profile 查看）
      </div>

      {photoOpen && <PhotoEvalDialog onClose={() => setPhotoOpen(false)} />}
    </div>
  )
}

function AchievementToast({ ids, onClose }: { ids: string[]; onClose: () => void }) {
  useEffect(() => {
    const t = setTimeout(onClose, 4000)
    return () => clearTimeout(t)
  }, [onClose])
  return (
    <div className="achievement-toast" onClick={onClose}>
      <div className="at-icon">🏆</div>
      <div className="at-body">
        <div className="at-title">解锁新成就 ×{ids.length}</div>
        <div className="at-sub">查看 Profile 了解详情</div>
      </div>
      <div className="at-close">×</div>
    </div>
  )
}

function badgeFromVenue(venueId: string): string {
  const map: Record<string, string> = {
    panda: 'panda',
    koala: 'koala',
    gorilla: 'gorilla',
    tiger: 'tiger',
    giraffe: 'giraffe',
    meerkat: 'meerkat',
    red_panda: 'red_panda',
    tangjiahe: 'tangjiahe',
  }
  return map[venueId] || ''
}