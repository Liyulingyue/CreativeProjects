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
  const [photoOpen, setPhotoOpen] = useState(false)
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [openEntry, setOpenEntry] = useState<string | null>(null)
  const [nearest, setNearest] = useState<any>(null)
  const [locating, setLocating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { visited: checkedIn } = useVisitedVenues()

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    function onPhotoLogChanged() {
      setPhotoLog(loadPhotoLog())
    }
    window.addEventListener('zooguide:photoLogChanged', onPhotoLogChanged)
    return () => window.removeEventListener('zooguide:photoLogChanged', onPhotoLogChanged)
  }, [])

  function toggleEntry(id: string) {
    setOpenEntry((cur) => (cur === id ? null : id))
  }

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
      {/* ===== 顶部：今日统计 ===== */}
      <div className="activity-stats-grid">
        <StatCell value={photoLog.length} label="出片" />
        <StatCell value={checkedIn.size} label="已打卡" />
        <StatCell
          value={photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0}
          label="最高分"
        />
      </div>

      {/* ===== 打卡方式入口 ===== */}
      <div className="activity-section-title">
        <span>🎯 打卡方式</span>
      </div>
      <div className="entry-list">
        <EntryRow
          icon="📷"
          title="拍照打卡"
          subtitle="拍一张动物照片，AI 识别并自动记录"
          badge={photoLog.length > 0 ? `${photoLog.length} 张` : undefined}
          isOpen={openEntry === 'photo'}
          onClick={() => setPhotoOpen(true)}
        />
        <EntryRow
          icon="🛰️"
          title="GPS 定位"
          subtitle="看看我在园区哪里，附近有什么馆"
          badge={nearest ? `${nearest.results.length} 馆` : undefined}
          isOpen={openEntry === 'gps'}
          onClick={() => toggleEntry('gps')}
        />
        {openEntry === 'gps' && (
          <div className="entry-expanded">
            <button
              className="activity-locate-btn"
              onClick={locate}
              disabled={locating}
            >
              {locating ? '🛰️ 定位中…' : nearest ? '🛰️ 重新定位' : '🛰️ 开始定位'}
            </button>
            {error && <div className="error-banner" style={{ fontSize: 12, marginBottom: 8 }}>{error}</div>}
            {nearest && (
              <div style={{ marginTop: 8 }}>
                <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginBottom: 6 }}>
                  {nearest.in_park_estimate ? '✅ 在园区内' : '⚠️ 可能在园区外'} ·{' '}
                  距 {nearest.results[0]?.name}{' '}
                  {nearest.results[0]?.distance_m >= 1000
                    ? `${(nearest.results[0].distance_m / 1000).toFixed(1)}km`
                    : `${Math.round(nearest.results[0]?.distance_m || 0)}m`}
                </div>
                {nearest.results.slice(0, 5).map((r: any, i: number) => (
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
          </div>
        )}

        <EntryRow
          icon="🦒"
          title="必看馆快速打卡"
          subtitle="21 个明星馆一键打卡，不拍照也行"
          badge={mustSee.length > 0 ? `${mustSee.length} 馆` : undefined}
          isOpen={openEntry === 'quick'}
          onClick={() => toggleEntry('quick')}
        />
        {openEntry === 'quick' && (
          <div className="entry-expanded">
            <div className="activity-checkin-grid">
              {mustSee.map((v) => (
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

      {/* ===== 我的收集入口 ===== */}
      <div className="activity-section-title">
        <span>📚 我的收集</span>
      </div>
      <div className="entry-list">
        <PhotoLogEntry
          photos={photoLog}
          onClick={() => toggleEntry('photos')}
          isOpen={openEntry === 'photos'}
        />
        <BadgeLogEntry
          unlocked={unlockedBadges}
          onClick={() => toggleEntry('badges')}
          isOpen={openEntry === 'badges'}
        />
        <CheckinLogEntry
          checkedIn={checkedIn}
          venues={venues}
          onClick={() => toggleEntry('checkins')}
          isOpen={openEntry === 'checkins'}
        />
      </div>

      <div
        className="card"
        style={{
          marginTop: 14,
          fontSize: 12,
          color: 'var(--fg-muted)',
        }}
      >
        💡 未来扩展：音频导览、动物知识问答、活动报名等都会以同样入口形式加进来
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

function EntryRow({
  icon,
  title,
  subtitle,
  badge,
  isOpen,
  onClick,
}: {
  icon: string
  title: string
  subtitle: string
  badge?: string
  isOpen?: boolean
  onClick: () => void
}) {
  return (
    <button
      className={`entry-row ${isOpen ? 'open' : ''}`}
      onClick={onClick}
    >
      <div className="entry-row-icon">{icon}</div>
      <div className="entry-row-body">
        <div className="entry-row-title">{title}</div>
        <div className="entry-row-subtitle">{subtitle}</div>
      </div>
      {badge && <div className="entry-row-badge">{badge}</div>}
      <div className="entry-row-chevron">{isOpen ? '−' : '›'}</div>
    </button>
  )
}

function PhotoLogEntry({
  photos,
  onClick,
  isOpen,
}: {
  photos: PhotoLogEntry[]
  onClick: () => void
  isOpen?: boolean
}) {
  return (
    <>
      <button
        className={`entry-row ${isOpen ? 'open' : ''}`}
        onClick={onClick}
      >
        <div className="entry-row-icon">🖼</div>
        <div className="entry-row-body">
          <div className="entry-row-title">我的出片</div>
          <div className="entry-row-subtitle">所有拍照评价 + 徽章记录</div>
        </div>
        <div className="entry-row-badge">{photos.length} 张</div>
        <div className="entry-row-chevron">{isOpen ? '−' : '›'}</div>
      </button>
      {isOpen && (
        <div className="entry-expanded">
          {photos.length === 0 ? (
            <div className="activity-empty">还没有出片</div>
          ) : (
            <div className="entry-photo-list">
              {photos.map((p) => (
                <div key={p.evaluation_id} className="entry-photo-row">
                  <div className="entry-photo-emoji">{venueEmoji(p.matched_venue_id)}</div>
                  <div className="entry-photo-body">
                    <div className="entry-photo-name">{p.matched_venue_name}</div>
                    <div className="entry-photo-meta">
                      {p.animal_guess} · {p.badge} · {p.vibe_score}分
                    </div>
                  </div>
                  <div className="entry-photo-time">
                    {new Date(p.ts).toLocaleDateString('zh-CN', { month: 'numeric', day: 'numeric' })}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </>
  )
}

function BadgeLogEntry({
  unlocked,
  onClick,
  isOpen,
}: {
  unlocked: Set<string>
  onClick: () => void
  isOpen?: boolean
}) {
  const count = unlocked.size
  return (
    <>
      <button
        className={`entry-row ${isOpen ? 'open' : ''}`}
        onClick={onClick}
      >
        <div className="entry-row-icon">🏅</div>
        <div className="entry-row-body">
          <div className="entry-row-title">我的徽章</div>
          <div className="entry-row-subtitle">已解锁 / 总徽章</div>
        </div>
        <div className="entry-row-badge">{count} / {ALL_BADGES.length}</div>
        <div className="entry-row-chevron">{isOpen ? '−' : '›'}</div>
      </button>
      {isOpen && (
        <div className="entry-expanded">
          <div className="badge-grid">
            {ALL_BADGES.map((b) => {
              const isUnlocked = unlocked.has(b.id)
              return (
                <div
                  key={b.id}
                  className={`badge-tile ${isUnlocked ? '' : 'locked'}`}
                >
                  <div className="badge-tile-icon">{b.emoji}</div>
                  <div className="badge-tile-body">
                    <div className="badge-tile-name">{b.name}</div>
                    <div className={`badge-tile-status ${isUnlocked ? 'unlocked' : ''}`}>
                      {isUnlocked ? '✓ 已解锁' : '🔒 未解锁'}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </>
  )
}

function CheckinLogEntry({
  checkedIn,
  venues,
  onClick,
  isOpen,
}: {
  checkedIn: Set<string>
  venues: Venue[]
  onClick: () => void
  isOpen?: boolean
}) {
  const checked = venues.filter((v) => checkedIn.has(v.id))
  return (
    <>
      <button
        className={`entry-row ${isOpen ? 'open' : ''}`}
        onClick={onClick}
      >
        <div className="entry-row-icon">📍</div>
        <div className="entry-row-body">
          <div className="entry-row-title">打卡记录</div>
          <div className="entry-row-subtitle">所有已打卡的场馆</div>
        </div>
        <div className="entry-row-badge">{checkedIn.size} 馆</div>
        <div className="entry-row-chevron">{isOpen ? '−' : '›'}</div>
      </button>
      {isOpen && (
        <div className="entry-expanded">
          {checked.length === 0 ? (
            <div className="activity-empty">还没打卡过任何馆</div>
          ) : (
            <div className="entry-photo-list">
              {checked.map((v) => (
                <div key={v.id} className="entry-photo-row">
                  <div className="entry-photo-emoji">📍</div>
                  <div className="entry-photo-body">
                    <div className="entry-photo-name">{v.name}</div>
                    <div className="entry-photo-meta">{v.area}</div>
                  </div>
                  <button
                    className="entry-photo-undo"
                    onClick={(e) => {
                      e.stopPropagation()
                      const next = new Set(checkedIn)
                      next.delete(v.id)
                      saveVisited(next)
                    }}
                  >
                    ↶
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </>
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