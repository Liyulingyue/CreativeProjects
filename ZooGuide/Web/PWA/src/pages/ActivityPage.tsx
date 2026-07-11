import { useNavigate } from 'react-router-dom'

export interface ActivityDef {
  key: string
  label: string
  icon: string
  desc: string
  path: string
  gradient: string
}

export const ACTIVITIES: ActivityDef[] = [
  {
    key: 'photo',
    label: '拍照打卡',
    icon: '📷',
    desc: '拍下动物，AI 验证打卡',
    path: '/activity/photo',
    gradient: 'linear-gradient(135deg, #10b981, #059669)',
  },
  {
    key: 'wall',
    label: '出片评分',
    icon: '🌟',
    desc: '查看出片记录和评分',
    path: '/activity/wall',
    gradient: 'linear-gradient(135deg, #8b5cf6, #6d28d9)',
  },
  {
    key: 'gps',
    label: 'GPS 打卡',
    icon: '📍',
    desc: '定位附近场馆，一键打卡',
    path: '/activity/gps',
    gradient: 'linear-gradient(135deg, #0891b2, #0e7490)',
  },
]

export function ActivityPage() {
  const navigate = useNavigate()

  return (
    <div>
      {ACTIVITIES.map((a) => (
        <button
          key={a.key}
          className="activity-card"
          onClick={() => navigate(a.path)}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 14,
            width: '100%',
            padding: '14px 16px',
            marginBottom: 10,
            borderRadius: 14,
            border: '1px solid var(--border)',
            background: a.gradient,
            color: 'white',
            textAlign: 'left',
            cursor: 'pointer',
          }}
        >
          <div style={{ fontSize: 32 }}>{a.icon}</div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 16, fontWeight: 700 }}>{a.label}</div>
            <div style={{ fontSize: 12, opacity: 0.9, marginTop: 2 }}>{a.desc}</div>
          </div>
          <div style={{ fontSize: 18, opacity: 0.7 }}>›</div>
        </button>
      ))}
    </div>
  )
}
