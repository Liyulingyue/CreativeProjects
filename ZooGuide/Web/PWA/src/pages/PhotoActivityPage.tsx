import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import type { Meta, Venue } from '../types'
import { api } from '../api/client'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { loadPhotoLog, loadVisitedBySource, addVisitedSource, getCheckinPhoto, type PhotoLogEntry } from '../lib/storage'
import { useVisitedVenues } from '../hooks/useVisitedVenues'
import { venueEmoji } from '../lib/venue-helpers'

const ACTIVITY = 'photo'

interface Props {
  venues?: Venue[]
  meta?: Meta | null
}

export function PhotoActivityPage({ venues: venuesProp, meta }: Props) {
  const navigate = useNavigate()
  const [venues, setVenues] = useState<Venue[]>(venuesProp || [])
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const { visited, version } = useVisitedVenues()
  const [selectedVenue, setSelectedVenue] = useState<Venue | null>(null)
  const [showFlow, setShowFlow] = useState(false)
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)

  useEffect(() => {
    if (venuesProp) {
      setVenues(venuesProp)
    } else {
      api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    }
  }, [venuesProp])

  useEffect(() => {
    refreshData()
  }, [version])

  function handleVenueClick(v: Venue) {
    setSelectedVenue(v)
    setShowFlow(false)
  }

  function refreshData() {
    setPhotoLog(loadPhotoLog())
  }

  const maxVibe = photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.score)) : 0

  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const a = v.area || '场馆'
    if (!byArea[a]) byArea[a] = []
    byArea[a].push(v)
  })

  const checkinPhoto = selectedVenue ? getCheckinPhoto(selectedVenue.id) : undefined
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
              {list.filter((v) => loadVisitedBySource('photo').has(v.id)).length}/{list.length}
            </span>
          </div>
          {list.map((v) => {
            const vVisited = loadVisitedBySource('photo').has(v.id)
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
            {vPhoto && ` · ${vPhoto.score}分`}
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
        <div className="modal-mask" onClick={() => setSelectedVenue(null)} role="presentation">
          <div className="modal" onClick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
            <h3>{venueEmoji(selectedVenue.id)} {selectedVenue.name}</h3>
            <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 12 }}>
              {selectedVenue.animals.slice(0, 3).join(' · ')}
            </div>

            {isVisited && checkinPhoto && (
              <div style={{ marginBottom: 14 }}>
                <div style={{ fontSize: 12, color: 'var(--fg-muted)', marginBottom: 6 }}>打卡记录</div>
                <div
                  style={{
                    background: 'var(--primary-soft)',
                    borderRadius: 10,
                    padding: '10px 12px',
                    display: 'flex',
                    gap: 10,
                    alignItems: 'center',
                  }}
                >
                  {checkinPhoto.thumbnail && (
                    <img
                      src={checkinPhoto.thumbnail}
                      alt={checkinPhoto.animal || selectedVenue.name}
                      onClick={() => setPreviewUrl(checkinPhoto.preview || checkinPhoto.thumbnail || null)}
                      style={{
                        width: 56,
                        height: 56,
                        borderRadius: 8,
                        objectFit: 'cover',
                        flexShrink: 0,
                        cursor: 'pointer',
                      }}
                    />
                  )}
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <span style={{ fontSize: 13, fontWeight: 600 }}>{checkinPhoto.animal || selectedVenue.name}</span>
                      <span style={{ background: '#10b981', color: '#fff', borderRadius: 6, padding: '2px 8px', fontSize: 12, fontWeight: 700 }}>
                        {checkinPhoto.score}分
                      </span>
                    </div>
                    {checkinPhoto.desc && (
                      <div style={{ marginTop: 4, fontSize: 12, color: 'var(--fg-muted)', lineHeight: 1.4 }}>
                        {checkinPhoto.desc}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}

            {isVisited && !checkinPhoto && (
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
              addVisitedSource(venueId, 'photo')
              api.checkin(venueId, 'photo').catch(() => {})
              refreshData()
            }}
          />
        </div>
      )}

      {previewUrl && (
        <div
          className="modal-mask"
          onClick={() => setPreviewUrl(null)}
          style={{ background: 'rgba(0,0,0,0.9)', zIndex: 1000 }}
          role="presentation"
        >
          <img
            src={previewUrl}
            alt="预览"
            onClick={(e) => e.stopPropagation()}
            style={{
              maxWidth: '90vw',
              maxHeight: '85vh',
              borderRadius: 12,
              objectFit: 'contain',
            }}
          />
          <button
            onClick={() => setPreviewUrl(null)}
            style={{
              position: 'absolute',
              top: 16,
              right: 16,
              background: 'rgba(255,255,255,0.2)',
              color: '#fff',
              border: 'none',
              borderRadius: '50%',
              width: 36,
              height: 36,
              fontSize: 18,
              cursor: 'pointer',
            }}
          >
            ×
          </button>
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
