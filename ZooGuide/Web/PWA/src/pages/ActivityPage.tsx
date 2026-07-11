import { useEffect, useState } from 'react'
import type { Venue } from '../types'
import { api } from '../api/client'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { PhotoWallFlow } from '../components/flows/PhotoWallFlow'
import { GpsFlow } from '../components/flows/GpsFlow'
import { loadPhotoLog, type PhotoLogEntry } from '../lib/storage'

type FlowKind = 'photo' | 'wall' | 'gps' | null

export function ActivityPage() {
  const [flow, setFlow] = useState<FlowKind>(null)
  const [photoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const [venues, setVenues] = useState<Venue[]>([])

  useEffect(() => {
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
  }, [])

  // Full-screen flow overlay
  if (flow === 'photo') {
    return <PhotoFlow venues={venues} onClose={() => setFlow(null)} />
  }
  if (flow === 'wall') {
    return <PhotoWallFlow onClose={() => setFlow(null)} onOpenPhoto={() => setFlow('photo')} />
  }
  if (flow === 'gps') {
    return <GpsFlow onClose={() => setFlow(null)} onOpenPlan={() => { /* TODO */ }} />
  }

  // Main activity page (3 cards hub)
  const todayCount = photoLog.filter((p) => {
    const d = new Date(p.ts)
    const now = new Date()
    return d.toDateString() === now.toDateString()
  }).length
  const maxVibe = photoLog.length > 0 ? Math.max(...photoLog.map((p) => p.vibe_score)) : 0
  const unlockedBadges = new Set(photoLog.map((p) => badgeFromVenue(p.matched_venue_id)))

  return (
    <div>
      <div
        className="activity-card-main"
        onClick={() => setFlow('photo')}
        style={{ background: 'linear-gradient(135deg, #f59e0b, #d97706)' }}
      >
        <div className="acm-icon">📷</div>
        <div className="acm-body">
          <div className="acm-title">拍照打卡</div>
          <div className="acm-sub">
            选场馆 · 拍照 · AI 验证 · 自动记录
            <br />
            今日 {todayCount} 张 · 总 {photoLog.length} 张
          </div>
        </div>
        <div className="acm-arrow">›</div>
      </div>

      <div
        className="activity-card-main"
        onClick={() => setFlow('wall')}
        style={{ background: 'linear-gradient(135deg, #8b5cf6, #7c3aed)' }}
      >
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
        <div className="acm-arrow">›</div>
      </div>

      <div
        className="activity-card-main"
        onClick={() => setFlow('gps')}
        style={{ background: 'linear-gradient(135deg, #0891b2, #0e7490)' }}
      >
        <div className="acm-icon">📍</div>
        <div className="acm-body">
          <div className="acm-title">GPS 打卡</div>
          <div className="acm-sub">
            看看我在园区哪里
            <br />
            附近有什么馆可以打卡
          </div>
        </div>
        <div className="acm-arrow">›</div>
      </div>
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