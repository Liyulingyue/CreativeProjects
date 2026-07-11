import { useEffect, useState } from 'react'
import type { Meta, UserPreference, Venue } from '../types'
import { api } from '../api/client'
import { getStoredUser, type AuthUser } from '../lib/storage'

interface Props {
  meta: Meta | null
  venues: Venue[]
  prefs: UserPreference | null
  user: AuthUser | null
  hasRoute: boolean
  onStartPlan: () => void
  onContinueRoute: () => void
  onSwitchTab: (tab: string) => void
}

interface RouteSummary {
  id: string
  summary: string
  total_minutes: number
  created_at: string
}

export function HomePage({ meta, venues, prefs, user, hasRoute, onStartPlan, onContinueRoute, onSwitchTab }: Props) {
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
      {/* Hero */}
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

      {/* Quick actions */}
      <div className="quick-actions">
        <button className="qa-card" onClick={() => onSwitchTab('chat')}>
          <div className="qa-icon">💬</div>
          <div className="qa-title">跟 Agent 聊聊</div>
          <div className="qa-desc">说"想看熊猫/累了"</div>
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

      {/* Continue last route */}
      {(hasRoute || recentRoute) && (
        <div className="card" style={{ background: 'var(--primary-soft)', border: 'none' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div style={{ fontSize: 24 }}>🧭</div>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 600, color: 'var(--primary-strong)', fontSize: 14 }}>
                {hasRoute ? '当前有未完成的路线' : '最近规划的路线'}
              </div>
              <div style={{ fontSize: 11, color: 'var(--fg-muted)', marginTop: 2 }}>
                {recentRoute ? `${Math.round(recentRoute.total_minutes / 60 * 10) / 10}h · ${recentRoute.created_at?.slice(0, 10)}` : '点开继续'}
              </div>
            </div>
            <button
              className="btn btn-primary"
              style={{ padding: '8px 14px', fontSize: 13 }}
              onClick={onContinueRoute}
            >
              查看
            </button>
          </div>
        </div>
      )}

      {/* Stats (logged in) */}
      {stats && (
        <div className="card">
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