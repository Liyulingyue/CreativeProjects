import { useState } from 'react'
import { PhotoFlow } from '../components/flows/PhotoFlow'
import { PhotoWallFlow } from '../components/flows/PhotoWallFlow'
import { GpsFlow } from '../components/flows/GpsFlow'
import { useVisitedVenues } from '../hooks/useVisitedVenues'
import { saveVisited, loadPhotoLog, type PhotoLogEntry } from '../lib/storage'

type FlowKind = 'photo' | 'wall' | 'gps' | null

export function ActivityPage() {
  const [flow, setFlow] = useState<FlowKind>(null)
  const [photoLog] = useState<PhotoLogEntry[]>(loadPhotoLog())
  const { visited: checkedIn } = useVisitedVenues()

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

  // Full-screen flow overlay
  if (flow === 'photo') {
    return <PhotoFlow onClose={() => setFlow(null)} />
  }
  if (flow === 'wall') {
    return <PhotoWallFlow onClose={() => setFlow(null)} onOpenPhoto={() => setFlow('photo')} />
  }
  if (flow === 'gps') {
    return <GpsFlow onClose={() => setFlow(null)} onOpenPlan={() => { /* 跳到 PlanFlow 由上层处理 */ }} />
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
      {/* 3 main cards (click to open fullscreen flow) */}
      <div
        className="activity-card-main"
        onClick={() => setFlow('photo')}
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

      {/* 备选：必看馆快速打卡 (折叠) */}
      <details className="activity-alt">
        <summary>不拍照？21 个必看馆一键打卡</summary>
        <div className="activity-checkin-grid">
          {Array.from({ length: 21 })
            .map((_, i) => MUST_SEE_IDS[i])
            .filter(Boolean)
            .map((vid) => (
              <MustSeeTile
                key={vid.id}
                venueId={vid.id}
                name={vid.name}
                checkedIn={checkedIn}
                onClick={() => quickCheckin(vid.id)}
              />
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
    </div>
  )
}

// Hardcoded must-see list (synced with backend venues.json must_see)
const MUST_SEE_IDS = [
  { id: 'panda', name: '大熊猫馆' },
  { id: 'gorilla', name: '大猩猩馆' },
  { id: 'tiger', name: '虎馆' },
  { id: 'giraffe', name: '长颈鹿馆' },
  { id: 'koala', name: '考拉馆' },
  { id: 'meerkat', name: '细尾獴馆' },
  { id: 'red_panda', name: '小熊猫馆' },
  { id: 'tangjiahe', name: '唐家河展区' },
  { id: 'asian_elephant', name: '亚洲象馆' },
  { id: 'orangutan', name: '猩猩馆' },
  { id: 'kangaroo', name: '澳洲袋鼠角' },
  { id: 'lemur', name: '马岛客厅' },
  { id: 'rhino', name: '犀牛领地' },
  { id: 'china_cat', name: '中国猫科馆' },
  { id: 'cat_planet', name: '猫科星球' },
  { id: 'asian_primates', name: '亚洲灵长区' },
  { id: 'wolf', name: '狼馆' },
  { id: 'bear', name: '熊馆' },
  { id: 'monkey_mountain', name: '猴山' },
  { id: 'hornbill', name: '犀鸟馆' },
  { id: 'crane', name: '鹤园' },
]

function MustSeeTile({
  venueId,
  name,
  checkedIn,
  onClick,
}: {
  venueId: string
  name: string
  checkedIn: Set<string>
  onClick: () => void
}) {
  return (
    <button
      className={`activity-checkin-tile ${checkedIn.has(venueId) ? 'on' : ''}`}
      onClick={onClick}
    >
      <div className="activity-checkin-name">{name}</div>
      <div className={`activity-checkin-mark ${checkedIn.has(venueId) ? 'on' : ''}`}>
        {checkedIn.has(venueId) ? '✓' : '+'}
      </div>
    </button>
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