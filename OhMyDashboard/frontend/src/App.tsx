import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { Header } from './components/Header'
import { Dashboard } from './pages/Dashboard'
import { Docker } from './pages/Docker'
import { Startup } from './pages/Startup'
import { Process } from './pages/Process'
import { Network } from './pages/Network'

function App() {
  return (
    <BrowserRouter>
      <div className="p-6 bg-white min-h-screen text-slate-800">
        <Header />
        
        <main>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/docker" element={<Docker />} />
            <Route path="/startup" element={<Startup />} />
            <Route path="/process" element={<Process />} />
            <Route path="/network" element={<Network />} />
          </Routes>
        </main>

        <footer className="mt-20 py-8 border-t border-slate-100 flex flex-col items-center gap-4">
          <p className="text-slate-400 text-sm">Powered by React + Vite + FastAPI</p>
        </footer>
      </div>
    </BrowserRouter>
  )
}

export default App

