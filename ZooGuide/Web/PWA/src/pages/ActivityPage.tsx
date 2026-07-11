import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoEvalDialog } from '../components/PhotoEvalDialog'
import { useVisitedVenues } from '../hooks/useVisitedVenues'
import { saveVisited, loadPhotoLog, type PhotoLogEntry } from '../lib/storage'

const ALL_BADGES = [
  { id: 'panda', emoji: '🐼', name: '国宝认证' },
  { id: 'gorilla', emoji: '🦍', name: '野菜F4' },
  { id: 'tiger', emoji: '🐯', name: '百兽之王' },
  { id: 'giraffe', emoji: '🦒', name: '长颈代表' },
  { id: 'koala', emoji: '🐨', name: '睡眠代言' },
  { id: 'meerkat', emoji: '🦊', name: '站岗小队长' },
  { id: 'red_panda', emoji: '🐾', name: '撞脸不撞DNA' },
  { id: 'tangjiahe', emoji: '🏔️', name: '首发游客' },
]

export function ActivityPage() {
  const [venues, setVenues] = useState<Venue[]>([])
  const [nearest, setNearest] = useState<any>(null)
  const [locating, setLocating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [photoOpen, setPhotoOpen] = useState(false)
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [showAlt, setShowAlt] = useState(false)
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
    fetch('/api/checkin', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ venue_id: venueId }),
    }).catch(() => {})
  }

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

  const mustSee = venues.filter((v) => v.must_see)
  const unlockedBadges = new Set(photoLog.map((p) => badgeFromVenue(p.matched_venue_id)))

  return (
    <div>
      {/* ===== Hero: 拍照打卡 ===== */}
      <div className="activity-hero">
        <div className="activity-hero-icon">📸</div>
        <div className="activity-hero-title">拍照打卡</div>
        <div className="activity-hero-sub">
          拍一张动物照片 → AI 识别 → 自动打卡 + 出徽章
        </div>
        <button className="activity-cta-camera" onClick={() => setPhotoOpen(true)}>
          📷 来一张
        </button>
      </div>

      {/* ===== 统计 ===== */}
      <div className="activity-stats-grid">
        <StatCell value={photoLog.length} label="出片" />
        <StatCell value={checkedIn.size} label="已打卡" />
        <StatCell
          value={photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0}
          label="最高分"
        />
      </div>

      {/* ===== 我的出片 (横向滚动缩略图) ===== */}
      <div className="activity-section-title">
        🖼 我的出片
        <span className="activity-section-count">{photoLog.length} 张</span>
      </div>
      {photoLog.length === 0 ? (
        <div className="activity-empty">还没有出片，去拍第一张试试 ✨</div>
      ) : (
        <div className="activity-photos">
          {photoLog.slice(0, 12).map((p) => (
            <div key={p.evaluation_id} className="activity-photo">
              <div className="activity-photo-emoji">
                {venueEmoji(p.matched_venue_id)}
              </div>
              <div className="activity-photo-name">{p.matched_venue_name}</div>
              <div className="activity-photo-badge">{p.badge}</div>
            </div>
          ))}
        </div>
      )}

      {/* ===== 我的徽章 ===== */}
      <div className="activity-section-title">
        🏅 我的徽章
        <span className="activity-section-count">
          {unlockedBadges.size} / {ALL_BADGES.length}
        </span>
      </div>
      <div className="badge-grid">
        {ALL_BADGES.map((b) => {
          const unlocked = unlockedBadges.has(b.id)
          return (
            <div
              key={b.id}
              className={`badge-tile ${unlocked ? '' : 'locked'}`}
            >
              <div className="badge-tile-icon">{b.emoji}</div>
              <div className="badge-tile-body">
                <div className="badge-tile-name">{b.name}</div>
                <div className={`badge-tile-status ${unlocked ? 'unlocked' : ''}`}>
                  {unlocked ? '✓ 已解锁' : '🔒 未解锁'}
                </div>
              </div>
            </div>
          )
        })}
      </div>

      {/* ===== 备选工具 (折叠) ===== */}
      <div className="activity-alt-section" style={{ marginTop: 14 }}>
        <div className="activity-alt-header">
          <div className="activity-alt-title">不拍照也能打卡？</div>
          <button
            className="activity-alt-toggle"
            onClick={() => setShowAlt(!showAlt)}
          >
            {showAlt ? '−' : '+'}
          </button>
        </div>

        {showAlt && (
          <div className="activity-alt-content">
            <p>📍 自动定位 · 必看馆网格快速打卡</p>

            <button
              className="activity-locate-btn"
              onClick={locate}
              disabled={locating}
            >
              {locating ? '🛰️ 定位中…' : '🛰️ 自动定位当前位置'}
            </button>

            {error && (
              <div className="error-banner" style={{ marginBottom: 10, fontSize: 12 }}>
                {error}
              </div>
            )}

            {nearest && (
              <div style={{ marginBottom: 12 }}>
                {nearest.results.slice(0, 3).map((r: any, i: number) => (
                  <button
                    key={r.id}
                    className="nearest-row"
                    onClick={() => quickCheckin(r.id)}
                  >
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                      }}
                    >
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

            <div
              style={{
                fontSize: 11,
                fontWeight: 700,
                color: 'var(--primary-strong)',
                marginBottom: 6,
              }}
            >
              ⭐ 必看馆快速打卡
            </div>
            <div className="activity-checkin-grid">
              {mustSee.slice(0, 8).map((v) => (
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
          </div>
        )}
      </div>

      <div
        className="card"
        style={{
          marginTop: 14,
          fontSize: 12,
          color: 'var(--fg-muted)',
        }}
      >
        💡 提示：每张出片都会算分（0-100），80+ 解锁对应馆徽章
      </div>

      {photoOpen && <PhotoEvalDialog onClose={() => setPhotoOpen(false)} />}
    </div>
  )
}

function StatCell({ value, label }: { value: number; label: string }) {
  return (
    <div className="activity-stat-cell">
      <div className="activity-stat-num">{value}</div>
      <div className="activity-stat-label">{label}</div>
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

function venueEmoji(venueId: string): string {
  const map: Record<string, string> = {
    panda: '🐼',
    koala: '🐨',
    gorilla: '🦍',
    tiger: '🐯',
    giraffe: '🦒',
    meerkat: '🦊',
    red_panda: '🐾',
    tangjiahe: '🏔️',
  }
  return map[venueId] || '🐾'
}