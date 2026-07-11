interface Tab {
  id: string
  label: string
  icon: string
}

interface Props {
  active: string
  onChange: (tab: string) => void
}

const TABS: Tab[] = [
  { id: 'plan', label: '规划', icon: '🧭' },
  { id: 'nearby', label: '附近', icon: '📍' },
  { id: 'photo', label: '出片', icon: '📸' },
  { id: 'me', label: '我的', icon: '👤' },
]

export function TabBar({ active, onChange }: Props) {
  return (
    <nav className="tab-bar">
      {TABS.map((t) => {
        const isActive = active === t.id
        return (
          <button
            key={t.id}
            className={`tab-bar-btn ${isActive ? 'active' : ''}`}
            onClick={() => onChange(t.id)}
          >
            <span className="tab-bar-icon">{t.icon}</span>
            <span className="tab-bar-label">{t.label}</span>
          </button>
        )
      })}
    </nav>
  )
}