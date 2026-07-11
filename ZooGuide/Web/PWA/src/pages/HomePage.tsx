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
      {/* Top route banner - shown only when active route exists */}
      {hasRoute && route && (
        <ActiveRouteCard
          route={route}
          onContinue={onContinueRoute}
          onClear={onClearRoute}
          onReplan={onStartPlan}
        />
      )}

      {/* Hero card - STATE AWARE */}
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

      {/* Quick actions - always available */}
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

      {/* Recent route from DB (logged in, no active route) */}
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

      {/* Stats (logged in) */}
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

      {/* Park info */}
      {meta && (
        <div className="card">
          <h3 className="card-title">📋 园区速览</h3>
          <div className="meta-info">
            <div className="item">🕒 {meta.open_time}–{meta.close_time}</div>
            <div className="item">🎫 {meta.ticket}</div>
            <div className="item">📍 {venues.length} 个展馆</div>
            <div className="item">📐 {Object.keys(meta.areas).length} 大片区</div>
          </div>
          <div style={{ marginTop: 10, fontSize: 12, color: 'var(--fg-muted)', lineHeight: 1.6 }}>
            {meta.highlights?.[0]}
          </div>
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

  const visitedInRoute = route.stops.filter((s) => visited.has(s.venue_id)).length
  const total = route.stops.length
  const progress = total > 0 ? visitedInRoute / total : 0

  return (
    <div className="active-route-card">
      <div className="arc-header">
        <div className="arc-pulse" />
        <span style={{ fontSize: 11, fontWeight: 700, color: 'var(--primary-strong)', letterSpacing: 0.5 }}>
          路线进行中
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

      <div className="arc-summary" onClick={onContinue}>
        <div className="arc-icon">🧭</div>
        <div style={{ flex: 1 }}>
          <div
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: 'var(--primary-strong)',
              lineHeight: 1.4,
            }}
          >
            {route.summary?.slice(0, 50) || '当前路线'}
          </div>
          <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginTop: 4 }}>
            {total} 馆 · {Math.round(route.total_minutes / 60 * 10) / 10}h
            {visitedInRoute > 0 && ` · 已打卡 ${visitedInRoute}`}
          </div>
        </div>
        <div className="arc-cta">查看 →</div>
      </div>

      {/* Progress bar */}
      {visitedInRoute > 0 && (
        <div className="arc-progress">
          <div
            className="arc-progress-bar"
            style={{ width: `${progress * 100}%` }}
          />
        </div>
      )}

      {/* Stops preview */}
      <div className="arc-stops">
        {route.stops.slice(0, 5).map((s, i) => (
          <div
            key={`${s.venue_id}-${i}`}
            className={`arc-stop ${visited.has(s.venue_id) ? 'visited' : ''}`}
            onClick={onContinue}
          >
            <span className="arc-stop-num">{i + 1}</span>
            <span className="arc-stop-name">{s.venue_name}</span>
            <span className="arc-stop-time">
              {s.arrive_time?.slice(0, 5) || ''}
            </span>
            {visited.has(s.venue_id) && <span className="arc-stop-mark">✓</span>}
          </div>
        ))}
        {route.stops.length > 5 && (
          <div className="arc-stops-more">…还有 {route.stops.length - 5} 馆</div>
        )}
      </div>

      <button className="btn btn-primary btn-full" style={{ marginTop: 12 }} onClick={onContinue}>
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
      <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--primary-strong)' }}>
        {value}
      </div>
      <div style={{ fontSize: 11, color: 'var(--fg-muted)' }}>{label}</div>
    </div>
  )
}