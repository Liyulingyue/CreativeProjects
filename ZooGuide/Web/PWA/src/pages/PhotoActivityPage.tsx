import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { loadPhotoLog, loadActivityVisited, saveActivityVisited, type PhotoLogEntry } from '../lib/storage'

const ACTIVITY = 'photo'

export function PhotoActivityPage() {
  const navigate = useNavigate()
  const [venues, setVenues] = useState<Venue[]>([])
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [visited, setVisited] = useState<Set<string>>(loadActivityVisited(ACTIVITY))
  const [selectedVenue, setSelectedVenue] = useState<Venue | null>(null)
  const [showFlow, setShowFlow] = useState(false)

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
  }, [])

  function handleVenueClick(v: Venue) {
    setSelectedVenue(v)
    setShowFlow(false)
  }

  function refreshData() {
    setVisited(loadActivityVisited(ACTIVITY))
    setPhotoLog(loadPhotoLog())
  }

  const maxVibe = photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0

  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const a = v.area || '场馆'
    if (!byArea[a]) byArea[a] = []
    byArea[a].push(v)
  })

  const venuePhotos = selectedVenue
    ? photoLog.filter((p) => p.matched_venue_id === selectedVenue.id)
    : []
  const isVisited = selectedVenue ? visited.has(selectedVenue.id) : false

  return (
    <div className="fullscreen-flow">
      <header className="flow-header">
        <button className="flow-back" onClick={() => navigate('/activity')}>←</button>
        <div className="flow-title">📷 拍照打卡</div>
        <div style={{ width: 36 }} />
      </header>

      <div className="flow-body">
        <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
          <MiniStat value={visited.size} label="已打卡" highlight />
          <MiniStat value={photoLog.length} label="出片" />
          <MiniStat value={maxVibe} label="最高分" />
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
            const vVisited = visited.has(v.id)
            const vPhoto = photoLog.find((p) => p.matched_venue_id === v.id)
            return (
              <button
                key={v.id}
                className={`venue-list-item ${vVisited ? 'visited' : ''}`}
                onClick={() => handleVenueClick(v)}
              >
                <div className="venue-list-emoji">{venueEmoji(v.id)}</div>
                <div className="venue-list-body">
                  <div className="venue-list-name">{v.name}</div>
                  <div className="venue-list-meta">
                    {v.animals.slice(0, 2).join(' · ')}
                    {vPhoto && ` · ${vPhoto.vibe_score}分`}
                  </div>
                </div>
                <div className="venue-list-status">
                  {vVisited ? (
                    <span className="venue-list-checked">✓ 已游览</span>
                  ) : (
                    <span className="venue-list-tap">去拍照 ›</span>
                  )}
                </div>
              </button>
            )
          })}
        </div>
        ))}
      </div>

      {selectedVenue && !showFlow && (
        <div className="modal-mask" onClick={() => setSelectedVenue(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{venueEmoji(selectedVenue.id)} {selectedVenue.name}</h3>
            <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 12 }}>
              {selectedVenue.animals.slice(0, 3).join(' · ')}
            </div>

            {isVisited && venuePhotos.length > 0 && (
              <div style={{ marginBottom: 14 }}>
                <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 6 }}>打卡记录</div>
                {venuePhotos.map((p) => (
                  <div
                    key={p.evaluation_id}
                    style={{
                      background: 'var(--primary-soft)',
                      borderRadius: 10,
                      padding: '10px 12px',
                      marginBottom: 6,
                    }}
                  >
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <span style={{ fontSize: 13 }}>{p.animal_guess}</span>
                      <span style={{ background: '#10b981', color: '#fff', borderRadius: 6, padding: '2px 8px', fontSize: 12, fontWeight: 700 }}>
                        {p.vibe_score}分
                      </span>
                    </div>
                    <div style={{ display: 'inline-block', marginTop: 4, background: 'var(--accent)', color: '#fff', borderRadius: 6, padding: '2px 8px', fontSize: 12 }}>
                      🏅 {p.badge}
                    </div>
                    <div style={{ marginTop: 4, fontSize: 12, color: 'var(--fg-muted)' }}>
                      「{p.caption}」
                    </div>
                  </div>
                ))}
              </div>
            )}

            {isVisited && venuePhotos.length === 0 && (
              <div style={{ marginBottom: 14, padding: 12, background: 'var(--primary-soft)', borderRadius: 10, textAlign: 'center', color: 'var(--primary-strong)', fontSize: 13 }}>
                ✓ 已打卡
              </div>
            )}

            <div className="modal-actions">
              <button className="btn btn-ghost btn-full" onClick={() => setSelectedVenue(null)}>
                关闭
              </button>
              <button className="btn btn-primary btn-full" onClick={() => setShowFlow(true)}>
                📷 {isVisited ? '再拍一张' : '去拍照'}
              </button>
            </div>
          </div>
        </div>
      )}

      {selectedVenue && showFlow && (
        <div className="flow-modal-overlay">
          <PhotoFlow
            venue={selectedVenue}
            onClose={() => {
              setShowFlow(false)
              setSelectedVenue(null)
              refreshData()
            }}
            onCheckinSuccess={(venueId) => {
              const next = new Set(loadActivityVisited(ACTIVITY))
              next.add(venueId)
              saveActivityVisited(ACTIVITY, next)
              refreshData()
            }}
          />
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
    panda: '🐼',
    koala: '🐨',
    gorilla: '🦍',
    tiger: '🐯',
    giraffe: '🦒',
    meerkat: '🦝',
    red_panda: '🐾',
    tangjiahe: '🏔️',
    hornbill: '🦜',
    crane: '🦢',
    monkey_mountain: '🐒',
    bear: '🐻',
  }
  return map[venueId] || '📍'
}
