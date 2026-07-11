import { useEffect, useState } from 'react'
import { api } from './api/client'
import type { Meta, UserPreference, Venue } from './types'
import { TabBar } from './components/TabBar'
import { PlanPage } from './pages/PlanPage'
import { NearbyPage } from './pages/NearbyPage'
import { PhotoPage } from './pages/PhotoPage'
import { ProfilePage } from './pages/ProfilePage'
import { getStoredUser, loadPrefs } from './lib/storage'
import type { AuthUser } from './lib/storage'

type Tab = 'plan' | 'nearby' | 'photo' | 'me'

export default function App() {
  const [tab, setTab] = useState<Tab>('plan')
  const [prefs, setPrefs] = useState<UserPreference | null>(null)
  const [meta, setMeta] = useState<Meta | null>(null)
  const [venues, setVenues] = useState<Venue[]>([])
  const [user, setUser] = useState<AuthUser | null>(getStoredUser())

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    const saved = loadPrefs()
    if (saved) setPrefs(saved)
  }, [])

  const titles: Record<Tab, string> = {
    plan: '🧭 规划路线',
    nearby: '📍 附近场馆',
    photo: '📸 出片',
    me: user ? `👤 ${user.display_name}` : '👤 我的',
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>🦒 ZooGuide</h1>
        <span className="badge">红山省力 Agent</span>
        <span style={{ flex: 1, fontSize: 13, opacity: 0.9, textAlign: 'right', minWidth: 0, marginLeft: 8, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {titles[tab]}
        </span>
      </header>

      <main className="app-body">
        {tab === 'plan' && (
          <PlanPage initialPrefs={prefs} venues={venues} meta={meta} user={user} />
        )}
        {tab === 'nearby' && <NearbyPage />}
        {tab === 'photo' && <PhotoPage />}
        {tab === 'me' && (
          <ProfilePage user={user} onUserChange={setUser} />
        )}
      </main>

      <TabBar active={tab} onChange={(t) => setTab(t as Tab)} />
    </div>
  )
}