import { useEffect, useState } from 'react'
import { api } from './api/client'
import type { Meta, Route, UserPreference, Venue } from './types'
import { TabBar } from './components/TabBar'
import { PlanFlow } from './components/PlanFlow'
import { HomePage } from './pages/HomePage'
import { ChatPage } from './pages/ChatPage'
import { ActivityPage } from './pages/ActivityPage'
import { ProfilePage } from './pages/ProfilePage'
import { getStoredUser, loadPrefs } from './lib/storage'
import type { AuthUser } from './lib/storage'

type Tab = 'home' | 'chat' | 'activity' | 'me'

export default function App() {
  const [tab, setTab] = useState<Tab>('home')
  const [prefs, setPrefs] = useState<UserPreference | null>(null)
  const [meta, setMeta] = useState<Meta | null>(null)
  const [venues, setVenues] = useState<Venue[]>([])
  const [route, setRoute] = useState<Route | null>(null)
  const [user, setUser] = useState<AuthUser | null>(getStoredUser())
  const [planOpen, setPlanOpen] = useState(false)
  const [planInitialStage, setPlanInitialStage] = useState<'quiz' | undefined>(undefined)

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    const saved = loadPrefs()
    if (saved) setPrefs(saved)
    // Try restore route from localStorage
    try {
      const raw = localStorage.getItem('zooguide:currentRoute:v1')
      if (raw) setRoute(JSON.parse(raw))
    } catch {}
  }, [])

  // Persist current route
  useEffect(() => {
    try {
      if (route) localStorage.setItem('zooguide:currentRoute:v1', JSON.stringify(route))
      else localStorage.removeItem('zooguide:currentRoute:v1')
    } catch {}
  }, [route])

  const titles: Record<Tab, string> = {
    home: 'ZooGuide',
    chat: '红山导游',
    activity: '园区活动',
    me: user ? user.display_name : '我的',
  }

  function handleTabChange(t: string) {
    setTab(t as Tab)
  }

  function openPlan() {
    setPlanOpen(true)
    setPlanInitialStage(undefined)
  }

  function openPlanAtQuiz() {
    setPlanInitialStage('quiz')
    setPlanOpen(true)
  }

  function closePlan() {
    setPlanOpen(false)
    setPlanInitialStage(undefined)
  }

  function clearRoute() {
    setRoute(null)
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>🦒 ZooGuide</h1>
        <span className="badge">红山省力 Agent</span>
      </header>

      <main className="app-body">
        {tab === 'home' && (
          <HomePage
            meta={meta}
            venues={venues}
            prefs={prefs}
            user={user}
            route={route}
            hasRoute={!!route}
            onStartPlan={openPlan}
            onContinueRoute={openPlan}
            onReplanFromScratch={openPlanAtQuiz}
            onSwitchTab={handleTabChange}
            onClearRoute={clearRoute}
          />
        )}
        {tab === 'chat' && (
          <ChatPage
            currentRoute={route}
            prefs={prefs}
            onRouteUpdate={setRoute}
            onGoPlan={openPlan}
            onGoActivity={() => setTab('activity')}
          />
        )}
        {tab === 'activity' && <ActivityPage />}
        {tab === 'me' && <ProfilePage user={user} onUserChange={setUser} />}
      </main>

      {planOpen && (
        <PlanFlow
          initialPrefs={prefs}
          externalRoute={route}
          initialStage={planInitialStage}
          onClose={closePlan}
          onRouteChange={setRoute}
          onOpenChat={() => setTab('chat')}
        />
      )}

      <TabBar active={tab} onChange={handleTabChange} />
    </div>
  )
}