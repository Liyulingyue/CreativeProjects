import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { PhotoWallFlow } from '../components/flows/PhotoWallFlow'
import { GpsFlow } from '../components/flows/GpsFlow'
import { loadPhotoLog, loadVisited, saveVisited, type PhotoLogEntry } from '../lib/storage'

type TabKey = 'photo' | 'wall' | 'gps'

const TABS: { key: TabKey; label: string; icon: string }[] = [
  { key: 'photo', label: '拍照打卡', icon: '📷' },
  { key: 'wall', label: '出片评分', icon: '🌟' },
  { key: 'gps', label: 'GPS 打卡', icon: '📍' },
]

export function ActivityPage() {
  const [tab, setTab] = useState<TabKey>('photo')
  const [venues, setVenues] = useState<Venue[]>([])
  const [photoLog, setPhotoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [visited, setVisited] = useState<Set<string>>(loadVisited())
  const [selectedVenue, setSelectedVenue] = useState<Venue | null>(null)

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
  }, [])

  // Open photo flow for a specific venue
  if (selectedVenue) {
    return (
      <PhotoFlow
        venue={selectedVenue}
        onClose={() => {
          setSelectedVenue(null)
          setVisited(loadVisited())
          setPhotoLog(loadPhotoLog())
        }}
        onCheckinSuccess={(venueId) => {
          const next = new Set(loadVisited())
          next.add(venueId)
          saveVisited(next)
          setVisited(next)
          setPhotoLog(loadPhotoLog())
        }}
      />
    )
  }

  if (tab === 'wall') {
    return <PhotoWallFlow onClose={() => setTab('photo')} onOpenPhoto={() => setTab('photo')} />
  }
  if (tab === 'gps') {
    return <GpsFlow onClose={() => setTab('photo')} onOpenPlan={() => { /* TODO */ }} />
  }

  // photo tab
  const todayCount = photoLog.filter((p) => {
    const d = new Date(p.ts)
    const now = new Date()
    return d.toDateString() === now.toDateString()
  }).length
  const maxVibe = photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0

  return (
    <div>
      {/* Tab bar */}
      <div className="activity-tabs">
        {TABS.map((t) => (
          <button
            key={t.key}
            className={`activity-tab ${tab === t.key ? 'on' : ''}`}
            onClick={() => setTab(t.key)}
          >
            <span className="activity-tab-icon">{t.icon}</span>
            <span className="activity-tab-label">{t.label}</span>
          </button>
        ))}
      </div>

      {/* Compact stats header */}
      <div
        style={{
          display: 'flex',
          gap: 8,
          marginBottom: 12,
        }}
      >
        <MiniStat value={visited.size} label="已打卡" highlight />
        <MiniStat value={photoLog.length} label="出片" />
        <MiniStat value={maxVibe} label="最高分" />
      </div>

      {/* Photo Tab: Venue list by area */}
      <PhotoTabContent
        venues={venues}
        visited={visited}
        onSelect={(v) => setSelectedVenue(v)}
        todayCount={todayCount}
        photoLog={photoLog}
      />
    </div>
  )
}

function PhotoTabContent({
  venues,
  visited,
  onSelect,
  todayCount,
  photoLog,
}: {
  venues: Venue[]
  visited: Set<string>
  onSelect: (v: Venue) => void
  todayCount: number
  photoLog: PhotoLogEntry[]
}) {
  const byArea: Record<string, Venue[]> = {}
  venues.forEach((v) => {
    const a = v.area || '其他'
    if (!byArea[a]) byArea[a] = []
    byArea[a].push(v)
  })

  return (
    <div>
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
            const photo = photoLog.find((p) => p.matched_venue_id === v.id)
            return (
              <button
                key={v.id}
                className={`venue-list-item ${isVisited ? 'visited' : ''}`}
                onClick={() => onSelect(v)}
              >
                <div className="venue-list-emoji">{venueEmoji(v.id)}</div>
                <div className="venue-list-body">
                  <div className="venue-list-name">{v.name}</div>
                  <div className="venue-list-meta">
                    {v.animals.slice(0, 2).join(' · ')}
                    {photo && ` · ${photo.vibe_score}分`}
                  </div>
                </div>
                <div className="venue-list-status">
                  {isVisited ? (
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
