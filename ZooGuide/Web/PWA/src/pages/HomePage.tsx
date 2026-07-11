import { useEffect, useState } from 'react'
import type { Meta, Route, UserPreference, Venue } from '../types'
import { api } from '../api/client'
import { type AuthUser } from '../lib/storage'

interface Props {
  meta: Meta | null
  venues: Venue[]
  prefs: UserPreference | null
  user: AuthUser | null
  route: Route | null
  hasRoute: boolean
  onStartPlan: () => void
  onContinueRoute: () => void
  onReplanFromScratch: () => void
  onSwitchTab: (tab: string) => void
  onClearRoute: () => void
}

interface RouteSummary {
  id: string
  summary: string
  total_minutes: number
  created_at: string
}

export function HomePage({
  meta,
  venues,
  prefs,
  user,
  route,
  hasRoute,
  onStartPlan,
  onContinueRoute,
  onReplanFromScratch,
  onSwitchTab,
  onClearRoute,
}: Props) {
  const [recentRoute, setRecentRoute] = useState<RouteSummary | null>(null)
  const [stats, setStats] = useState<any>(null)

  useEffect(() => {
    if (!user) return
    api
      .mySummary()
      .then((s) => {
        setStats(s.stats)
        if (s.recent_routes && s.recent_routes.length > 0) {
          setRecentRoute(s.recent_routes[0])
        }
      })
      .catch(() => {})
  }, [user?.id])

  return (
    <div>
      {hasRoute && route && (
          <ActiveRouteCard
            route={route}
            onContinue={onContinueRoute}
            onClear={onClearRoute}
            onReplan={onReplanFromScratch}
          />
      )}

      {/* Hero card */}
      {!hasRoute && (
        <div
          className="card"
          style={{
            background: 'linear-gradient(135deg, var(--primary-soft), #fff)',
            textAlign: 'center',
            padding: '24px 16px',
          }}
        >
          <div style={{ fontSize: 56, marginBottom: 6 }}>🦒</div>
          <h2 style={{ margin: '0 0 8px', color: 'var(--primary-strong)', fontSize: 22 }}>
            逛红山，不必人挤人
          </h2>
          <p
            style={{
              fontSize: 13,
              color: 'var(--fg-muted)',
              lineHeight: 1.6,
              margin: '0 auto 18px',
              maxWidth: 320,
            }}
          >
            告诉我你的时间、体力、带没带娃、怕不怕晒，
            我帮你定制一趟只属于你的红山路线。
          </p>
          <button className="btn btn-primary btn-full" onClick={onStartPlan}>
            ✨ 开始定制我的路线
          </button>
        </div>
      )}

      {/* Quick actions */}
      <div className="quick-actions">
        <button className="qa-card" onClick={() => onSwitchTab('chat')}>
          <div className="qa-icon">💬</div>
          <div className="qa-title">跟 Agent 聊聊</div>
          <div className="qa-desc">{hasRoute ? '调当前路线' : '说"想看熊猫"'}</div>
        </button>
        <button className="qa-card" onClick={() => onSwitchTab('activity')}>
          <div className="qa-icon">📍</div>
          <div className="qa-title">附近场馆</div>
          <div className="qa-desc">定位看周围</div>
        </button>
        <button className="qa-card" onClick={() => onSwitchTab('activity')}>
          <div className="qa-icon">📸</div>
          <div className="qa-title">出片彩蛋</div>
          <div className="qa-desc">拍照打分</div>
        </button>
        <button className="qa-card" onClick={() => onSwitchTab('me')}>
          <div className="qa-icon">👤</div>
          <div className="qa-title">我的</div>
          <div className="qa-desc">{user ? '看历史' : '登录'}</div>
        </button>
      </div>

      {/* Recent route from DB */}
      {user && recentRoute && !hasRoute && (
        <div
          className="card"
          style={{
            background: 'linear-gradient(135deg, #fef3c7, #fff)',
            cursor: 'pointer',
          }}
          onClick={onStartPlan}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div style={{ fontSize: 24 }}>🕰️</div>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 700, color: 'var(--primary-strong)', fontSize: 14 }}>
                上次规划的路线
              </div>
              <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginTop: 2 }}>
                {recentRoute.summary?.slice(0, 40)} ·{' '}
                {Math.round(recentRoute.total_minutes / 60 * 10) / 10}h ·{' '}
                {recentRoute.created_at?.slice(0, 10)}
              </div>
            </div>
            <span className="pill-btn primary">恢复 →</span>
          </div>
        </div>
      )}

      {/* Stats */}
      {stats && (
        <div
          className="card"
          style={{ cursor: 'pointer' }}
          onClick={() => onSwitchTab('me')}
        >
          <h3 className="card-title">📊 我的足迹</h3>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: 6 }}>
            <StatBlock label="打卡" value={stats.checkins_count} />
            <StatBlock label="馆" value={stats.venues_visited} />
            <StatBlock label="路线" value={stats.routes_planned} />
            <StatBlock label="出片" value={stats.photos_evaluated} />
          </div>
        </div>
      )}

      {/* About 红山 (only shown when no route, so it doesn't clutter) */}
      {!hasRoute && <AboutHongshan meta={meta} venues={venues} />}

      {/* Park quick facts (always shown - useful even with active route) */}
      {meta && <ParkFacts meta={meta} venues={venues} />}
    </div>
  )
}

/* ----- About 红山 section ----- */
function AboutHongshan({ meta, venues }: { meta: Meta | null; venues: Venue[] }) {
  if (!meta) return null

  return (
    <div className="card">
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12 }}>
        <div style={{ fontSize: 28 }}>🦒</div>
        <div>
          <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--primary-strong)' }}>
            {meta.name}
          </div>
          <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>{meta.name_en} · 国家AAAA级</div>
        </div>
      </div>

      <p
        style={{
          fontSize: 13,
          color: 'var(--fg)',
          lineHeight: 1.6,
          margin: '0 0 12px',
        }}
      >
        这是中国第一个取消动物表演的动物园（2011 年起）。不卖动物表演，不诱导投喂，
        一切都按动物的需求来设计展馆。游客能看到动物在自然的步调下生活。
      </p>

      <div
        style={{
          fontSize: 11,
          fontWeight: 700,
          color: 'var(--primary-strong)',
          marginBottom: 8,
          textTransform: 'uppercase',
          letterSpacing: 0.5,
        }}
      >
        🏆 为什么是红山
      </div>

      <div className="highlight-list">
        {(meta.highlights || []).map((h, i) => (
          <div key={i} className="highlight-item">
            <div className="highlight-num">{i + 1}</div>
            <div className="highlight-text">{h}</div>
          </div>
        ))}
      </div>
    </div>
  )
}

/* ----- Park facts (quick info grid + tips) ----- */
function ParkFacts({ meta, venues }: { meta: Meta; venues: Venue[] }) {
  const mustSeeCount = venues.filter((v) => v.must_see).length
  const areaEntries = Object.entries(meta.areas || {})
  const address = meta.address || ''

  return (
    <div className="card">
      <h3 className="card-title">📋 园区速览</h3>

      {/* Quick facts grid */}
      <div className="meta-info">
        <div className="item">
          🕒 {meta.open_time}–{meta.close_time}
        </div>
        <div className="item">
          🎫 {meta.ticket}
        </div>
        <div className="item">
          📍 {venues.length} 个展馆
        </div>
        <div className="item">
          ⭐ {mustSeeCount} 个必看
        </div>
        <div className="item">
          📐 {areaEntries.length} 大片区
        </div>
        <div className="item">
          🗺️ {meta.area_km2} km²
        </div>
      </div>

      {/* Areas breakdown */}
      <div style={{ marginTop: 14 }}>
        <div
          style={{
            fontSize: 11,
            fontWeight: 700,
            color: 'var(--primary-strong)',
            marginBottom: 6,
            textTransform: 'uppercase',
            letterSpacing: 0.5,
          }}
        >
          🗺️ 四大片区
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          {areaEntries.map(([name, desc]) => (
            <div key={name} style={{ fontSize: 12, color: 'var(--fg)', lineHeight: 1.5 }}>
              <strong style={{ color: 'var(--primary-strong)' }}>· {name}</strong>
              <span style={{ color: 'var(--fg-muted)' }}>：{desc}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Tips */}
      <div
        style={{
          marginTop: 14,
          padding: '10px 12px',
          background: '#fef9e7',
          borderRadius: 10,
          fontSize: 12,
          color: '#78350f',
          lineHeight: 1.6,
        }}
      >
        <div style={{ fontWeight: 700, marginBottom: 4 }}>💡 实用小贴士</div>
        <div>
          · 上午 9-10 点动物最活跃；下午 2-3 点午睡醒来
        </div>
        <div>· 北门最近熊猫馆；南门是 2025 新区主入口</div>
        <div>· 山地型园区，多上下坡，穿舒适的鞋</div>
        <div>· 禁止投喂动物、禁止使用闪光灯、禁止无人机</div>
      </div>

      {/* Address */}
      {address && (
        <div
          style={{
            marginTop: 12,
            fontSize: 11,
            color: 'var(--fg-muted)',
            display: 'flex',
            alignItems: 'center',
            gap: 4,
          }}
        >
          📍 {address}
        </div>
      )}
    </div>
  )
}

function ActiveRouteCard({
  route,
  onContinue,
  onClear,
  onReplan,
}: {
  route: Route
  onContinue: () => void
  onClear: () => void
  onReplan: () => void
}) {
  const visited = (() => {
    try {
      const raw = localStorage.getItem('zooguide:visited:v1')
      return new Set(raw ? JSON.parse(raw) : [])
    } catch {
      return new Set()
    }
  })()

  // 找当前所在馆（优先从 localStorage 读取，否则第一个未游览的）
  let currentIdx: number
  try {
    const saved = localStorage.getItem(`zooguide:currentStop:${route.id}`)
    if (saved) {
      const idx = parseInt(saved, 10)
      if (!isNaN(idx) && idx >= 0 && idx < route.stops.length) {
        currentIdx = idx
      } else {
        currentIdx = route.stops.findIndex((s) => !visited.has(s.venue_id))
        currentIdx = currentIdx === -1 ? 0 : currentIdx
      }
    } else {
      currentIdx = route.stops.findIndex((s) => !visited.has(s.venue_id))
      currentIdx = currentIdx === -1 ? 0 : currentIdx
    }
  } catch {
    currentIdx = 0
  }

  const stops = route.stops
  const total = stops.length
  const currentStop = stops[Math.min(currentIdx, total - 1)]
  const nextStop = stops[currentIdx + 1]
  const visitedCount = stops.filter((s) => visited.has(s.venue_id)).length
  const remainingCount = total - visitedCount
  const progress = total > 0 ? visitedCount / total : 0

  return (
    <div className="active-route-card">
      {/* Header */}
      <div className="arc-header">
        <div className="arc-pulse" />
        <span style={{ fontSize: 11, fontWeight: 700, color: 'var(--primary-strong)', letterSpacing: 0.5 }}>
          路线进行中 · 第 {currentIdx + 1}/{total} 馆
        </span>
        <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
          <button
            className="arc-icon-btn"
            onClick={(e) => {
              e.stopPropagation()
              onReplan()
            }}
            title="重新规划"
          >
            🔄
          </button>
          <button
            className="arc-icon-btn danger"
            onClick={(e) => {
              e.stopPropagation()
              if (confirm('确定丢弃当前路线？')) {
                onClear()
              }
            }}
            title="丢弃当前路线"
          >
            ✕
          </button>
        </div>
      </div>

      {/* 当前馆 - 高亮 */}
      {currentStop && (
        <div className="arc-current-stop" onClick={onContinue}>
          <div className="arc-cs-label">📍 当前</div>
          <div className="arc-cs-name">{currentStop.venue_name}</div>
          <div className="arc-cs-time">
            🕐 {currentStop.arrive_time} – {currentStop.leave_time} ·{' '}
            {currentStop.visit_minutes}min
          </div>
        </div>
      )}

      {/* 下一站预览 */}
      {nextStop && (
        <div className="arc-next-stop" onClick={onContinue}>
          <span className="arc-ns-label">↓ 下一站 · 步行 {nextStop.walk_to_next_minutes}min</span>
          <span className="arc-ns-name">{nextStop.venue_name}</span>
          <span className="arc-ns-time"> {nextStop.arrive_time}</span>
        </div>
      )}

      {!nextStop && remainingCount === 0 && (
        <div className="arc-next-stop finish">
          🎉 全部游览完
        </div>
      )}
      {!nextStop && remainingCount > 0 && (
        <div className="arc-next-stop finish">
          最后一站·{currentStop.venue_name}
        </div>
      )}

      {/* 进度条 */}
      <div className="arc-progress" onClick={onContinue}>
        <div className="arc-progress-bar" style={{ width: `${progress * 100}%` }} />
        <div className="arc-progress-label">
          {visitedCount}/{total} 已游览{remainingCount > 0 && ` · 还剩 ${remainingCount} 馆`}
        </div>
      </div>

      {/* 主 CTA */}
      <button className="btn btn-primary btn-full" onClick={onContinue}>
        📍 打开完整路线
      </button>
    </div>
  )
}

function StatBlock({ label, value }: { label: string; value: number }) {
  return (
    <div
      style={{
        background: 'var(--primary-soft)',
        padding: '10px 6px',
        borderRadius: 8,
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--primary-strong)' }}>{value}</div>
      <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>{label}</div>
    </div>
  )
}