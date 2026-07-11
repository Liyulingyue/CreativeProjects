import { useEffect, useState } from 'react'
import { Routes, Route, useLocation, useNavigate } from 'react-router-dom'
import { api } from './api/client'
import type { Meta, Route as RouteType, UserPreference, Venue } from './types'
import { TabBar } from './components/TabBar'
import { PlanFlow } from './components/PlanFlow'
import { HomePage } from './pages/HomePage'
import { ChatPage } from './pages/ChatPage'
import { ActivityPage } from './pages/ActivityPage'
import { PhotoActivityPage } from './pages/PhotoActivityPage'
import { PhotoWallPage } from './pages/PhotoWallPage'
import { GpsFlowPage } from './pages/GpsFlowPage'
import { ProfilePage } from './pages/ProfilePage'
import { getStoredUser, loadPrefs } from './lib/storage'
import type { AuthUser } from './lib/storage'

type Tab = 'home' | 'chat' | 'activity' | 'me'

function getTabFromPath(pathname: string): Tab {
  if (pathname.startsWith('/chat')) return 'chat'
  if (pathname.startsWith('/activity')) return 'activity'
  if (pathname.startsWith('/me')) return 'me'
  return 'home'
}

export default function App() {
  const location = useLocation()
  const navigate = useNavigate()
  const tab = getTabFromPath(location.pathname)

  const [prefs, setPrefs] = useState<UserPreference | null>(null)
  const [meta, setMeta] = useState<Meta | null>(null)
  const [venues, setVenues] = useState<Venue[]>([])
  const [route, setRoute] = useState<RouteType | null>(null)
  const [user, setUser] = useState<AuthUser | null>(getStoredUser())
  const [planOpen, setPlanOpen] = useState(false)
  const [planInitialStage, setPlanInitialStage] = useState<'quiz' | undefined>(undefined)

  useEffect(() => {
    api.meta().then(setMeta).catch(console.error)
    api.venues().then((d) => setVenues(d.venues)).catch(console.error)
    const saved = loadPrefs()
    if (saved) setPrefs(saved)
    try {
      const raw = localStorage.getItem('zooguide:currentRoute:v1')
      if (raw) setRoute(JSON.parse(raw))
    } catch {}
  }, [])

  useEffect(() => {
    try {
      if (route) localStorage.setItem('zooguide:currentRoute:v1', JSON.stringify(route))
      else localStorage.removeItem('zooguide:currentRoute:v1')
    } catch {}
  }, [route])

  function handleTabChange(t: string) {
    const pathMap: Record<string, string> = { home: '/', chat: '/chat', activity: '/activity', me: '/me' }
    navigate(pathMap[t] || '/')
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

  const isActivitySubPage = location.pathname !== '/activity' && location.pathname.startsWith('/activity/')

  return (
    <div className="app">
      {!isActivitySubPage && (
        <header className="app-header">
          <h1>🦒 ZooGuide</h1>
          <span className="badge">红山省力 Agent</span>
        </header>
      )}

      <main className="app-body">
        <Routes>
          <Route path="/" element={
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
              onClearRoute={() => setRoute(null)}
            />
          } />
          <Route path="/chat" element={
            <ChatPage
              currentRoute={route}
              prefs={prefs}
              onRouteUpdate={setRoute}
              onGoPlan={openPlan}
              onGoActivity={() => navigate('/activity')}
            />
          } />
          <Route path="/activity" element={<ActivityPage />} />
          <Route path="/activity/photo" element={<PhotoActivityPage />} />
          <Route path="/activity/wall" element={<PhotoWallPage />} />
          <Route path="/activity/gps" element={<GpsFlowPage />} />
          <Route path="/me" element={
            <ProfilePage user={user} onUserChange={setUser} />
          } />
        </Routes>
      </main>

      {planOpen && (
        <PlanFlow
          initialPrefs={prefs}
          externalRoute={route}
          initialStage={planInitialStage}
          onClose={closePlan}
          onRouteChange={setRoute}
          onOpenChat={() => navigate('/chat')}
        />
      )}

      {!isActivitySubPage && <TabBar active={tab} onChange={handleTabChange} />}
    </div>
  )
}
